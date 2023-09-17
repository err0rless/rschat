use std::io::Write;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::{broadcast, mpsc},
};

use crate::{client::command::*, db, packet::*};

pub mod command;
pub mod session;

enum HandleCommandStatus {
    // Requested to exit program
    Exit,

    // Continue to handle
    Continue,
}

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
        let msg_str = match incoming_rx.recv().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        match serde_json::from_str::<Message>(msg_str.as_str()) {
            Ok(msg) if msg.is_system => println!("[#System] {}", msg.msg),
            Ok(msg) => println!("{}{}: {}", get_mark(&msg.id), msg.id, msg.msg),
            _ => (),
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
) -> HandleCommandStatus {
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
            let Some(user) = db::user::User::from_stdin().await else {
                println!("[#System] failed to register");
                return HandleCommandStatus::Continue;
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
                return HandleCommandStatus::Continue;
            }

            let Some(login_info) = db::user::Login::from_stdin(login_id).await else {
                println!("[#System] `id` or `password` is empty");
                return HandleCommandStatus::Continue;
            };

            // id backup
            let id_clone = login_info.id.clone().unwrap();
            if let Err(e) = outgoing_tx
                .send(LoginReq { login_info }.as_json_string())
                .await
            {
                println!("[login] Channel send failed, try again: '{}'", e);
                return HandleCommandStatus::Continue;
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
                    return HandleCommandStatus::Continue;
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
                return HandleCommandStatus::Continue;
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
        Some(Command::Goto(channel_name)) => {
            _ = outgoing_tx
                .send(GotoReq { channel_name }.as_json_string())
                .await;
            match consume_til::<GotoRes>(incoming_tx.subscribe()).await.result {
                Ok(name) => {
                    // goto succeeded, change channel
                    state.channel = name.clone();
                    println!("[#System:Goto] succeeded to join channel '{}'", name)
                }
                Err(e) => println!("[#System:Goto] failed to join channel: '{}'", e),
            }
        }
        Some(Command::Exit) => {
            println!(" >> See you soon <<");
            _ = outgoing_tx.send(Exit {}.as_json_string()).await;
            return HandleCommandStatus::Exit;
        }
        // Not a command
        None => (),
    }
    HandleCommandStatus::Continue
}

async fn handle_chat(outgoing_tx: &mpsc::Sender<String>, msg: &str, state: &session::State) {
    let msg_bytes = Message {
        id: state.id.clone(),
        msg: msg.to_owned(),
        is_system: false,
    }
    .as_json_string();

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
        print!(
            "You ({}{} in '{}') >> ",
            get_mark(&state.id),
            state.id,
            state.channel
        );
        std::io::stdout().flush().unwrap();

        let mut buf: Vec<u8> = Vec::new();
        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());

        _ = reader.read_until(b'\n', &mut buf).await;
        buf.pop();

        let msg = match String::from_utf8(buf) {
            Ok(s) if s.is_empty() => continue,
            Ok(s) => s,
            Err(_) => continue,
        };

        if msg.starts_with('/') {
            // exit if return value of handle_command is true
            if let HandleCommandStatus::Exit =
                handle_command(&outgoing_tx, &incoming_tx, &msg, &mut state).await
            {
                return;
            }
        } else {
            handle_chat(&outgoing_tx, &msg, &state).await;
        }
    }
}

/// Consumes broadcast channel until encounter the packet type `P`
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

pub async fn run_client(port: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("|----------------------------------------------|");
    println!("|--------------- [RsSimpleChat] ---------------|");
    println!("|----------------------------------------------|");

    // Establish a connection and split into two unidirectional streams
    let (rd, wr) = match TcpStream::connect(format!("0.0.0.0:{}", port)).await {
        Ok(s) => tokio::io::split(s),
        Err(e) => panic!("'{}'", e),
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

    let id = {
        outgoing_tx
            .send(
                LoginReq {
                    // You are a guest when once join the server
                    login_info: db::user::Login::guest(),
                }
                .as_json_string(),
            )
            .await?;

        match consume_til::<LoginRes>(incoming_tx.subscribe())
            .await
            .result
        {
            Ok(r) => r,
            Err(s) => panic!("{}", s),
        }
    };

    let state = session::State {
        is_guest: true,
        id,
        channel: "public".to_owned(),
    };

    // Shell-like interface for chat client
    let main_task = tokio::task::spawn(chat_interface(
        outgoing_tx.clone(),
        incoming_tx.clone(),
        state,
    ));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n >> Program Interrupted (Ctrl+C), See you soon <<");
            _ = outgoing_tx.send(Exit {}.as_json_string()).await;
            std::process::exit(-1);
        }
        _ = main_task => (),
    }
    Ok(())
}
