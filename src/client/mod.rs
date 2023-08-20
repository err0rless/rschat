use std::io::Write;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};

use crate::client::command::*;
use crate::db;
use crate::packet::*;

pub mod command;
pub mod session;

fn get_mark(id: &str) -> char {
    match id {
        s if s.starts_with("guest_") => '%',
        s if s.starts_with("root") => '#',
        _ => '@',
    }
}

/// receive formatted packets from `rd` and enqueue them to `incoming_tx` channel
async fn produce_incomings(mut rd: ReadHalf<TcpStream>, incoming_tx: broadcast::Sender<String>) {
    loop {
        // Size header
        let size_msg = match rd.read_u32().await {
            Ok(0) | Err(_) => panic!("[#System] EOF"),
            Ok(size) => size,
        };

        // Message body
        let mut buf = vec![0; size_msg as usize];
        let n = match rd.read_exact(buf.as_mut_slice()).await {
            Ok(0) | Err(_) => panic!("[#System] EOF"),
            Ok(size) => size,
        };

        let msg_str = String::from_utf8(buf[0..n].to_vec()).unwrap();
        _ = incoming_tx.send(msg_str);
    }
}

/// handle message packets
async fn print_message_packets(mut incoming_rx: broadcast::Receiver<String>) {
    loop {
        if let Ok(msg_str) = incoming_rx.recv().await {
            match serde_json::from_str::<Message>(msg_str.as_str()) {
                Ok(msg) if msg.is_system => println!("[#System] {}", msg.msg),
                Ok(msg) => println!("{}{}: {}", get_mark(&msg.id), msg.id, msg.msg),
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
    cmd: &str,
    state: &mut session::State,
) -> bool {
    match Command::from_str(cmd) {
        Some(Command::Help) => {
            Command::help();
        }
        Some(Command::Get(item)) => match &item[..] {
            "info" | "name" => {
                println!("[#cmd:get] Your ID: '{}'", state.id);
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
            if !state.is_guest {
                println!("[#System] You are already logged in");
                return false;
            }

            let login_info = if let Some(u) = db::user::Login::from_stdin(login_id).await {
                u
            } else {
                println!("[#System] `id` or `password` is empty");
                return false;
            };

            // id backup
            let id_clone = login_info.id.clone().unwrap();
            if let Err(e) = outgoing_tx
                .send(LoginReq { login_info }.as_json_string())
                .await
            {
                println!("[login] Channel send failed, try again: '{}'", e);
                return false;
            }

            // block til Login response
            match consume_til::<LoginRes>(incoming_tx.subscribe())
                .await
                .result
            {
                Ok(_) => {
                    // Succeded to login, you are no longer a guest
                    state.id = id_clone;
                    state.is_guest = false;
                    println!("[#System:Login] Success!");
                }
                Err(s) => println!("[#System:Login] Failure: '{}'", s),
            };
        }
        Some(Command::Fetch(fetch)) => {
            let item_str = match fetch {
                Fetch::UserList => "list",
                _ => {
                    println!("[fetch] Unhandled fetch item");
                    return false;
                }
            };

            if let Err(e) = outgoing_tx
                .send(
                    FetchReq {
                        item: item_str.to_owned(),
                    }
                    .as_json_string(),
                )
                .await
            {
                println!("[fetch] Channel send failed, try again: '{}'", e);
                return false;
            }

            // block til Login response
            let fetch_res = consume_til::<FetchRes>(incoming_tx.subscribe()).await;
            match fetch_res.item.as_str() {
                "list" => {
                    _ = fetch_res
                        .result
                        .map(|v| println!("{}", serde_json::to_string_pretty(&v).unwrap()))
                        .map_err(|e| println!("{}", e));
                }
                unknown => println!("[#System:Fetch] unknown item: '{}'", unknown),
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

async fn handle_chat(outgoing_tx: &mpsc::Sender<String>, msg: &str, state: &session::State) {
    let msg_bytes = Message {
        id: state.id.clone(),
        msg: msg.to_owned(),
        is_system: false,
    }
    .as_json_string();

    // send message to server
    if let Err(e) = outgoing_tx.send(msg_bytes).await {
        println!("Channel send failed: {}", e);
    }
}

async fn chat_interface(
    outgoing_tx: mpsc::Sender<String>,
    incoming_tx: broadcast::Sender<String>,
    mut state: session::State,
) {
    loop {
        print!("You ({}{}) >> ", get_mark(&state.id), state.id,);
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
            if handle_command(&outgoing_tx, &incoming_tx, &msg, &mut state).await {
                return;
            }
        } else {
            handle_chat(&outgoing_tx, &msg, &state).await;
        }
    }
}

/// Consumes broadcast channel until encounter the packet type: `T`
///
/// Make sure no same type of request between request and consume_til.
async fn consume_til<P>(mut incoming_rx: broadcast::Receiver<String>) -> P
where
    P: serde::de::DeserializeOwned,
{
    loop {
        if let Ok(msg) = incoming_rx.recv().await {
            let j: serde_json::Value = serde_json::from_str(msg.as_str()).unwrap();
            if let Ok(res) = serde_json::from_value::<P>(j) {
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
    tokio::task::spawn(print_message_packets(incoming_tx.subscribe()));

    // Try joining as a guest
    let id = {
        outgoing_tx
            .send(
                LoginReq {
                    login_info: db::user::Login::guest(),
                }
                .as_json_string(),
            )
            .await?;

        // blocks until server respond to the join request
        match consume_til::<LoginRes>(incoming_tx.subscribe())
            .await
            .result
        {
            Ok(r) => r,
            Err(s) => panic!("{}", s),
        }
    };

    let state = session::State { is_guest: true, id };

    // Shell-like interface for chat client
    tokio::task::spawn(chat_interface(
        outgoing_tx.clone(),
        incoming_tx.clone(),
        state,
    ))
    .await?;
    Ok(())
}
