use std::io::{self, Write};
use std::str::from_utf8;

use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;

use command::command::*;
use error::*;
use packet::packet::*;

mod command;
mod error;
mod packet;

type ErrorBox = Box<dyn std::error::Error>;

fn read_id() -> Result<String, ErrorBox> {
    print!(">> Enter your ID: ");
    io::stdout().flush().unwrap();

    let mut id = String::new();
    io::stdin().read_line(&mut id)?;
    id.pop();

    Ok(id)
}

async fn message_receiver(mut rd: ReadHalf<TcpStream>) {
    let mut buf = [0; 1024];
    loop {
        let n = match rd.read(&mut buf).await {
            Ok(0) => {
                println!("[#System] EOF");
                return;
            }
            Ok(size) => size,
            Err(_) => return,
        };

        let msg_str = from_utf8(&buf[0..n]).unwrap();
        if let Ok(msg) = serde_json::from_str::<Message>(msg_str) {
            if msg.is_system {
                println!("[#System] {}", msg.msg);
            } else {
                println!("@{}: {}", msg.id, msg.msg);
            }
        }
    }
}

async fn message_sender(mut wr: WriteHalf<TcpStream>, id: String) {
    loop {
        let mut msg = String::new();

        print!("type >> ");
        io::stdout().flush().unwrap();
        let _ = io::stdin().read_line(&mut msg);
        msg.pop();

        if msg.is_empty() {
            continue;
        }

        if msg.starts_with('/') {
            // message that starts with '/' is recognized as a command
            match Command::from_str(&msg) {
                Some(Command::Help) => {
                    Command::help();
                }
                Some(Command::Exit) => {
                    println!(" >> See you soon <<");
                    _ = wr
                        .write_all(Exit {}.into_json().to_string().as_bytes())
                        .await;
                    break;
                }
                Some(Command::Get(item)) => match &item[..] {
                    "info" | "name" => {
                        println!("[#cmd:get] Your ID: '{}'", id);
                    }
                    _ => {
                        println!("[#SystemError] Unknown item: '{}'", item);
                    }
                },
                // Not a command
                None => (),
            }
        } else {
            let msg_json_bytes = Message {
                id: id.clone(),
                msg: msg.clone(),
                is_system: false,
            }
            .as_json_bytes();

            // send message to server
            _ = wr.write_all(&msg_json_bytes).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), ErrorBox> {
    println!("|----------------------------------------------|");
    println!("|--------------- [RsSimpleChat] ---------------|");
    println!("|----------------------------------------------|");

    let stream = TcpStream::connect("0.0.0.0:8080").await?;
    let (mut rd, mut wr) = tokio::io::split(stream);

    // read id from stdin and request to join
    let join_res = match read_id() {
        Ok(id) => {
            // send Join request
            let join_req = Join { id: id.clone() }.into_json();
            let _ = wr.write_all(join_req.to_string().as_bytes()).await;

            // await JoinResult response
            let mut buf = [0; 1024];
            let ret: Result<String, String>;
            loop {
                if let Ok(n) = rd.read(&mut buf).await {
                    match PacketType::from_str(from_utf8(&buf[..n]).unwrap()) {
                        Some(PacketType::JoinResult(r)) if r.result => {
                            ret = Ok(id);
                            break;
                        }
                        Some(PacketType::JoinResult(r)) => {
                            ret = Err(r.msg);
                            break;
                        }
                        // Different type of packet received
                        _ => continue,
                    }
                }
            }
            ret
        }
        Err(_) => Err(format!("Failed to read ID from stdin")),
    };

    let id = match join_res {
        Ok(s) => {
            // Succeeded to connect to server
            _ = wr
                .write_all(Connected {}.into_json().to_string().as_bytes())
                .await;
            s
        }
        Err(e) => {
            eprintln!("err: {}", e);
            return Err(Box::new(ClientErr::JoinErr) as ErrorBox);
        }
    };
    println!("[#System] Hello '{}', Welcome to RsChat", id);

    // Interface for receiving broadcast messages from the server and print them
    tokio::task::spawn(message_receiver(rd));

    // Interface for communicating with server
    tokio::task::spawn(message_sender(wr, id.clone())).await?;
    Ok(())
}
