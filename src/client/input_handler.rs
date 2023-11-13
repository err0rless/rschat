use std::str::FromStr;

use tokio::sync::{broadcast, mpsc};

use crate::{
    client::{command::*, input_controller, session, util},
    db,
    packet::*,
};

pub enum HandleCommandStatus {
    // Requested to exit program
    Exit,

    // Continue to handle
    Continue,
}

pub async fn handle_command(
    outgoing_tx: &mpsc::Sender<String>,
    incoming_rx: broadcast::Receiver<String>,
    cmd: &str,
    out_msg_queue: input_controller::MessageChannel,
    state: &mut session::State,
) -> HandleCommandStatus {
    match Command::from_str(cmd) {
        Ok(Command::Help) => {
            Command::help();
        }
        Ok(Command::Get(item)) => match &item[..] {
            "info" | "name" => {
                out_msg_queue.push("System".to_owned(), format!("Your ID: '{}'", state.id));
            }
            _ => {
                out_msg_queue.push(
                    "SystemError".to_owned(),
                    format!("Unknown item: '{}'", item),
                );
            }
        },
        Ok(Command::Register) => {
            let Some(user) = db::user::User::from_stdin().await else {
                out_msg_queue.push("SystemError".to_owned(), "failed to register".to_owned());
                return HandleCommandStatus::Continue;
            };

            let register_req = RegisterReq { user }.as_json_string();
            if let Err(e) = outgoing_tx.send(register_req).await {
                out_msg_queue.push(
                    "SystemError".to_owned(),
                    format!("Channel send failed, retry later: {}", e),
                );
            }

            // block til Register response
            out_msg_queue.push(
                "System".to_owned(),
                match util::consume_til::<RegisterRes>(incoming_rx).await.result {
                    Ok(_) => "Success!".to_owned(),
                    Err(s) => format!("Failure: {}", s),
                },
            );
        }
        Ok(Command::Login(login_id)) => {
            if !state.is_guest {
                out_msg_queue.push(
                    "SystemError".to_owned(),
                    "You are already logged in".to_owned(),
                );
                return HandleCommandStatus::Continue;
            }

            let Some(login_info) = db::user::Login::from_stdin(login_id).await else {
                out_msg_queue.push(
                    "SystemError".to_owned(),
                    "`id` or `password` is empty".to_owned(),
                );
                return HandleCommandStatus::Continue;
            };

            // id backup
            let id_clone = login_info.id.clone().unwrap();
            if let Err(e) = outgoing_tx
                .send(LoginReq { login_info }.as_json_string())
                .await
            {
                out_msg_queue.push(
                    "SystemError".to_owned(),
                    format!("Channel send failed, try again: '{}'", e),
                );
                return HandleCommandStatus::Continue;
            }

            // block til Login response
            match util::consume_til::<LoginRes>(incoming_rx).await.result {
                Ok(_) => {
                    // Succeded to login, you are no longer a guest
                    state.id = id_clone;
                    state.is_guest = false;
                    out_msg_queue.push("System".to_owned(), "Success!".to_owned());
                }
                Err(s) => out_msg_queue.push("SystemError".to_owned(), format!("Failure: '{}'", s)),
            };
        }
        Ok(Command::Fetch(fetch)) => {
            let item_str = match fetch {
                Fetch::UserList => "list",
                _ => {
                    out_msg_queue.push("SystemError".to_owned(), "Unhandled fetch item".to_owned());
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
                out_msg_queue.push(
                    "SystemError".to_owned(),
                    format!("Channel send failed, try again: '{}'", e),
                );
                return HandleCommandStatus::Continue;
            }

            // block til Login response
            let fetch_res = util::consume_til::<FetchRes>(incoming_rx).await;
            match fetch_res.item.as_str() {
                "list" => match fetch_res.result {
                    Ok(v) => out_msg_queue.push(
                        "System".to_owned(),
                        serde_json::to_string_pretty(&v).unwrap(),
                    ),
                    Err(e) => out_msg_queue.push("SystemError".to_owned(), e),
                },
                unknown => out_msg_queue.push(
                    "SystemError".to_owned(),
                    format!("unknown item: '{}'", unknown),
                ),
            };
        }
        Ok(Command::Goto(channel_name)) => {
            _ = outgoing_tx
                .send(GotoReq { channel_name }.as_json_string())
                .await;
            match util::consume_til::<GotoRes>(incoming_rx).await.result {
                Ok(name) => {
                    // goto succeeded, change channel
                    state.channel = name.clone();
                    out_msg_queue.push(
                        "System".to_owned(),
                        format!("succeeded to join channel: '{}'", name),
                    );
                }
                Err(e) => out_msg_queue.push(
                    "SystemError".to_owned(),
                    format!("failed to join channel: '{}'", e),
                ),
            }
        }
        Ok(Command::Exit) => {
            out_msg_queue.push("System".to_owned(), " >> See You Soon << ".to_owned());
            _ = outgoing_tx.send(Exit {}.as_json_string()).await;
            return HandleCommandStatus::Exit;
        }
        // Not a command
        Err(ParseCommandError::UnknownCommand(cmd)) => out_msg_queue.push(
            "SystemError".to_owned(),
            format!("Unknown command: {}", cmd),
        ),
        Err(e) => out_msg_queue.push("SystemError".to_owned(), format!("{:?}", e)),
    }
    HandleCommandStatus::Continue
}
