use std::io::Write;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};

use crate::client::command::*;
use crate::db;
use crate::packet::packet::*;

async fn produce_incomings(mut rd: ReadHalf<TcpStream>, incoming_tx: broadcast::Sender<String>) {
    let mut buf = [0; 1024];
    loop {
        let n = match rd.read(&mut buf).await {
            Ok(0) | Err(_) => {
                println!("[#System] EOF");
                return;
            }
            Ok(size) => size,
        };

        let msg_str = String::from_utf8(buf[0..n].to_vec()).unwrap();
        _ = incoming_tx.send(msg_str);
    }
}

async fn handle_incoming_message(mut incoming_rx: broadcast::Receiver<String>) {
    loop {
        if let Ok(msg_str) = incoming_rx.recv().await {
            match serde_json::from_str::<Message>(msg_str.as_str()) {
                Ok(msg) if msg.is_system => println!("[#System] {}", msg.msg),
                Ok(msg) => println!("@{}: {}", msg.id, msg.msg),
                _ => (),
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

async fn handle_command(
    outgoing_tx: &mpsc::Sender<String>,
    incoming_tx: &broadcast::Sender<String>,
    cmd: &String,
    id: &mut String,
) -> bool {
    match Command::from_str(&cmd) {
        Some(Command::Help) => {
            Command::help();
        }
        Some(Command::Get(item)) => match &item[..] {
            "info" | "name" => {
                println!("[#cmd:get] Your ID: '{}'", id);
            }
            _ => {
                println!("[#SystemError] Unknown item: '{}'", item);
            }
        },
        Some(Command::Register) => {
            let user = if let Some(u) = db::user::User::from_stdin().await {
                u
            } else {
                println!("[#System] failed to register");
                return false;
            };

            let register_req = RegisterReq { user }.as_json_string();
            if let Err(e) = outgoing_tx.send(register_req).await {
                println!("[register] Channel send failed, retry later: {}", e);
            }

            // block til Register response
            match consume_til::<RegisterRes>(incoming_tx.subscribe())
                .await
                .result
            {
                Ok(_) => println!("[#System:Register] Success!"),
                Err(s) => println!("[#System:Register] Failure: '{}'", s),
            };
        }
        Some(Command::Login(login_id)) => {
            let login_info = if let Some(u) = db::user::Login::from_stdin(login_id).await {
                u
            } else {
                println!("[#System] failed to login");
                return false;
            };

            // id backup
            let id_clone = login_info.id.clone();
            if let Err(e) = outgoing_tx
                .send(LoginReq { login_info }.as_json_string())
                .await
            {
                println!("[login] Channel send failed, retry later: {}", e);
            }

            // blokc til Login response
            match consume_til::<LoginRes>(incoming_tx.subscribe())
                .await
                .result
            {
                Ok(_) => {
                    *id = id_clone;
                    println!("[#System:Login] Success!");
                }
                Err(s) => println!("[#System:Login] Failure: '{}'", s),
            };
        }
        Some(Command::Exit) => {
            println!(" >> See you soon <<");
            _ = outgoing_tx.send(Exit {}.as_json_string()).await;
            return true;
        }
        // Not a command
        None => (),
    }
    false
}

async fn handle_chat(outgoing_tx: &mpsc::Sender<String>, msg: &String, id: &String) {
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

async fn chat_shell(
    outgoing_tx: mpsc::Sender<String>,
    incoming_tx: broadcast::Sender<String>,
    mut id: String,
) {
    loop {
        print!(
            "{}{} ",
            id,
            match id.as_str() {
                "root" => '#',
                "guest" => '%',
                _ => '@',
            }
        );
        std::io::stdout().flush().unwrap();

        let mut buf: Vec<u8> = Vec::new();
        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());

        _ = reader.read_until(b'\n', &mut buf).await;
        buf.pop();

        let msg = String::from_utf8(buf).unwrap();
        if msg.is_empty() {
            continue;
        } else if msg.starts_with('/') {
            // message that starts with '/' is recognized as a command
            //
            // exit if return value of handle_command is true
            if handle_command(&outgoing_tx, &incoming_tx, &msg, &mut id).await {
                return;
            }
        } else {
            handle_chat(&outgoing_tx, &msg, &id).await;
        }
    }
}

// Consumes broadcast channel until encounter the packet type: `T`
// Ignores packets other than `T`
async fn consume_til<T>(mut incoming_rx: broadcast::Receiver<String>) -> T
where
    T: serde::de::DeserializeOwned,
{
    loop {
        if let Ok(msg) = incoming_rx.recv().await {
            let j: serde_json::Value = serde_json::from_str(msg.as_str()).unwrap();
            if let Ok(res) = serde_json::from_value::<T>(j) {
                break res;
            }
        }
    }
}

pub async fn run_client(port: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("|----------------------------------------------|");
    println!("|--------------- [RsSimpleChat] ---------------|");
    println!("|----------------------------------------------|");

    // Establish a connection and split into two unidirectional streams
    let (rd, wr) = match TcpStream::connect(format!("0.0.0.0:{}", port)).await {
        Ok(s) => tokio::io::split(s),
        Err(e) => panic!("{}'", e),
    };

    // Channel for messages will be sent to the server
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(32);

    // Channel for messages received
    let (incoming_tx, _) = broadcast::channel::<String>(32);

    // Task for comsuming the outgoing channel
    tokio::task::spawn(consume_outgoings(wr, outgoing_rx));

    // Task for reading TcpStream and enqueueing the messages to the channel
    tokio::task::spawn(produce_incomings(rd, incoming_tx.clone()));

    // Task for receiving broadcast messages from server
    tokio::task::spawn(handle_incoming_message(incoming_tx.subscribe()));

    // Try joining as a guest
    let id = {
        outgoing_tx.send(GuestJoinReq {}.as_json_string()).await?;

        // blocks until server respond to the join request
        match consume_til::<GuestJoinRes>(incoming_tx.subscribe())
            .await
            .id
        {
            Ok(r) => r,
            Err(s) => panic!("{}", s),
        }
    };

    // Shell-like interface for chat client
    tokio::task::spawn(chat_shell(outgoing_tx.clone(), incoming_tx.clone(), id)).await?;
    Ok(())
}
