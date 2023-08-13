use std::io::Write;
use std::str::from_utf8;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::client::command::*;
use crate::client::error::*;
use crate::packet::packet::*;

type ErrorBox = Box<dyn std::error::Error>;

async fn async_read_line() -> String {
    _ = tokio::io::stdout().write_all(b"you >> ").await;
    _ = tokio::io::stdout().flush().await;

    let mut buf: Vec<u8> = Vec::new();
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());

    _ = reader.read_until(b'\n', &mut buf).await;
    buf.pop();

    String::from_utf8(buf).unwrap()
}

async fn read_id() -> Result<String, ErrorBox> {
    print!(">> Enter Your ID: ");
    std::io::stdout().flush().unwrap();
    Ok(async_read_line().await)
}

async fn recv_message(mut rd: ReadHalf<TcpStream>) {
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

async fn consume_outgoings(
    mut write_stream: WriteHalf<TcpStream>,
    mut outgoing_rx: mpsc::Receiver<String>,
) {
    while let Some(msg) = outgoing_rx.recv().await {
        _ = write_stream.write_all(msg.as_bytes()).await;
    }
}

async fn message_sender(outgoing_tx: mpsc::Sender<String>, id: String) {
    loop {
        print!("you >> ");
        std::io::stdout().flush().unwrap();

        let mut buf: Vec<u8> = Vec::new();
        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());

        _ = reader.read_until(b'\n', &mut buf).await;
        buf.pop();

        let msg = String::from_utf8(buf).unwrap();
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
                    _ = outgoing_tx.send(Exit {}.as_json_string()).await;
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
            let msg_bytes = Message {
                id: id.clone(),
                msg: msg.clone(),
                is_system: false,
            }
            .as_json_string();

            // send message to server
            if let Err(e) = outgoing_tx.send(msg_bytes).await {
                println!("Channel send failed: {}", e);
            }
        }
    }
}

pub async fn client_main(port: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("|----------------------------------------------|");
    println!("|--------------- [RsSimpleChat] ---------------|");
    println!("|----------------------------------------------|");

    // Establish a connection and split into two unidirectional streams
    let (mut rd, mut wr) = match TcpStream::connect(format!("0.0.0.0:{}", port)).await {
        Ok(s) => tokio::io::split(s),
        Err(e) => panic!("{}'", e),
    };

    // Try joining with provided ID
    let join_res: Result<String, String> = match read_id().await {
        Ok(id) => {
            // send Join request
            let _ = wr.write_all(&Join { id: id.clone() }.as_json_bytes()).await;

            // await JoinResult response
            let mut buf = vec![0; 1024];
            loop {
                if let Ok(n) = rd.read(&mut buf).await {
                    match PacketType::from_str(from_utf8(&buf[..n]).unwrap()) {
                        Some(PacketType::JoinResult(r)) => {
                            break if r.result { Ok(id) } else { Err(r.msg) }
                        }
                        // Different type of packet received
                        _ => continue,
                    }
                }
            }
        }
        Err(_) => Err(format!("Failed to read ID from stdin")),
    };

    let id = match join_res {
        Ok(s) => {
            println!("[#System] Hello '{}', Welcome to RsChat", s);
            s
        }
        Err(e) => {
            std::io::stderr().write_all(format!("Join Failed: {}", e).as_bytes())?;
            return Err(Box::new(ClientErr::JoinErr));
        }
    };

    // Channel for messages that are being sent
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(32);

    // Outgoing channel consumer task
    tokio::task::spawn(consume_outgoings(wr, outgoing_rx));

    // Interface for receiving broadcast messages from server and print them
    tokio::task::spawn(recv_message(rd));

    // Interface for communicating with server
    tokio::task::spawn(message_sender(outgoing_tx.clone(), id.clone())).await?;
    Ok(())
}
