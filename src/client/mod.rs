use std::{io::Write, str::FromStr};
use tokio::{
    io::AsyncBufReadExt,
    net::TcpStream,
    sync::{broadcast, mpsc},
};

use crate::{client::command::*, db, packet::*};

pub mod background_task;
pub mod command;
pub mod session;
pub mod util;

enum HandleCommandStatus {
    // Requested to exit program
    Exit,

    // Continue to handle
    Continue,
}

async fn handle_command(
    outgoing_tx: &mpsc::Sender<String>,
    incoming_tx: &broadcast::Sender<String>,
    cmd: &str,
    state: &mut session::State,
) -> HandleCommandStatus {
    match Command::from_str(cmd) {
        Ok(Command::Help) => {
            Command::help();
        }
        Ok(Command::Get(item)) => match &item[..] {
            "info" | "name" => {
                println!("[#cmd:get] Your ID: '{}'", state.id);
            }
            _ => {
                println!("[#SystemError] Unknown item: '{}'", item);
            }
        },
        Ok(Command::Register) => {
            let Some(user) = db::user::User::from_stdin().await else {
                println!("[#System] failed to register");
                return HandleCommandStatus::Continue;
            };

            let register_req = RegisterReq { user }.as_json_string();
            if let Err(e) = outgoing_tx.send(register_req).await {
                println!("[register] Channel send failed, retry later: {}", e);
            }

            // block til Register response
            match util::consume_til::<RegisterRes>(incoming_tx.subscribe())
                .await
                .result
            {
                Ok(_) => println!("[#System:Register] Success!"),
                Err(s) => println!("[#System:Register] Failure: '{}'", s),
            };
        }
        Ok(Command::Login(login_id)) => {
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
            match util::consume_til::<LoginRes>(incoming_tx.subscribe())
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
        Ok(Command::Fetch(fetch)) => {
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
            let fetch_res = util::consume_til::<FetchRes>(incoming_tx.subscribe()).await;
            match fetch_res.item.as_str() {
                "list" => match fetch_res.result {
                    Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
                    Err(e) => println!("{}", e),
                },
                unknown => println!("[#System:Fetch] unknown item: '{}'", unknown),
            };
        }
        Ok(Command::Goto(channel_name)) => {
            _ = outgoing_tx
                .send(GotoReq { channel_name }.as_json_string())
                .await;
            match util::consume_til::<GotoRes>(incoming_tx.subscribe())
                .await
                .result
            {
                Ok(name) => {
                    // goto succeeded, change channel
                    state.channel = name.clone();
                    println!("[#System:Goto] succeeded to join channel '{}'", name)
                }
                Err(e) => println!("[#System:Goto] failed to join channel: '{}'", e),
            }
        }
        Ok(Command::Exit) => {
            println!(" >> See you soon <<");
            _ = outgoing_tx.send(Exit {}.as_json_string()).await;
            return HandleCommandStatus::Exit;
        }
        // Not a command
        Err(_) => (),
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
            util::get_mark(&state.id),
            state.id,
            state.channel
        );
        std::io::stdout().flush().unwrap();

        let mut buf: Vec<u8> = Vec::new();
        _ = tokio::io::BufReader::new(tokio::io::stdin())
            .read_until(b'\n', &mut buf)
            .await;
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
    tokio::task::spawn(background_task::consume_outgoings(wr, outgoing_rx));

    // Task for reading TcpStream and enqueueing the messages to the channel
    tokio::task::spawn(background_task::produce_incomings(rd, incoming_tx.clone()));

    // Task for receiving broadcast messages from server
    tokio::task::spawn(background_task::print_message_packets(
        incoming_tx.subscribe(),
    ));

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

        match util::consume_til::<LoginRes>(incoming_tx.subscribe())
            .await
            .result
        {
            Ok(r) => r,
            Err(s) => panic!("{}", s),
        }
    };

    let state = session::State::new_guest(id.as_str());

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
