use std::str::FromStr;

use tokio::sync::{broadcast, mpsc};

use crate::{
    client::{command::*, input_controller::*, session, util},
    db,
    packet::*,
};

#[derive(PartialEq)]
pub enum HandleCommandStatus {
    // Requested to exit program
    Exit,

    // Continue to handle
    Continue,
}

/// App holds the state of the application
pub struct App {
    pub input_controller: InputController,
    pub outgoing_tx: mpsc::Sender<String>,
    pub incoming_tx: broadcast::Sender<String>,
    pub state: session::State,
}

impl App {
    pub fn new(
        outgoing_tx: mpsc::Sender<String>,
        incoming_tx: broadcast::Sender<String>,
        state: session::State,
    ) -> Self {
        Self {
            input_controller: InputController::default(),
            outgoing_tx,
            incoming_tx,
            state,
        }
    }

    // Send message to the outgoing channel
    pub async fn send_message(&self) {
        let msg_bytes = Message {
            id: self.state.id.clone(),
            msg: self.input_controller.input.clone(),
            is_system: false,
        }
        .as_json_string();
        _ = self.outgoing_tx.send(msg_bytes).await;
    }

    pub async fn handle_command(&mut self) -> HandleCommandStatus {
        match Command::from_str(&self.input_controller.input) {
            Ok(Command::Help) => Command::help(),
            Ok(Command::Get(item)) => match &item[..] {
                "info" | "name" => {
                    self.input_controller
                        .push_sys_msg(format!("Your ID: '{}'", self.state.id));
                }
                _ => {
                    self.input_controller
                        .push_sys_err(format!("Unknown item: '{}'", item));
                }
            },
            Ok(Command::Register) => {
                let Some(user) = db::user::User::from_stdin().await else {
                    self.input_controller
                        .push_sys_err("failed to register".to_owned());
                    return HandleCommandStatus::Continue;
                };

                let register_req = RegisterReq { user }.as_json_string();
                if let Err(e) = self.outgoing_tx.send(register_req).await {
                    self.input_controller
                        .push_sys_err(format!("Channel send failed, retry later: {}", e));
                }

                // block til Register response
                self.input_controller.push_sys_msg(
                    match util::consume_til::<RegisterRes>(self.incoming_tx.subscribe())
                        .await
                        .result
                    {
                        Ok(_) => "Success!".to_owned(),
                        Err(s) => format!("Failure: {}", s),
                    },
                );
            }
            Ok(Command::Login(login_id)) => {
                if !self.state.is_guest {
                    self.input_controller
                        .push_sys_err("You are already logged in".to_owned());
                    return HandleCommandStatus::Continue;
                }

                let Some(login_info) = db::user::Login::from_stdin(login_id).await else {
                    self.input_controller
                        .push_sys_err("`id` or `password` is empty".to_owned());
                    return HandleCommandStatus::Continue;
                };

                // id backup
                let id_clone = login_info.id.clone().unwrap();
                if let Err(e) = self
                    .outgoing_tx
                    .send(LoginReq { login_info }.as_json_string())
                    .await
                {
                    self.input_controller
                        .push_sys_err(format!("Channel send failed, try again: '{}'", e));
                    return HandleCommandStatus::Continue;
                }

                // block til Login response
                match util::consume_til::<LoginRes>(self.incoming_tx.subscribe())
                    .await
                    .result
                {
                    Ok(_) => {
                        // Succeded to login, you are no longer a guest
                        self.state.id = id_clone;
                        self.state.is_guest = false;
                        self.input_controller.push_sys_msg("Success!".to_owned());
                    }
                    Err(s) => self
                        .input_controller
                        .push_sys_err(format!("Failure: '{}'", s)),
                };
            }
            Ok(Command::Fetch(fetch)) => {
                let item_str = match fetch {
                    Fetch::UserList => "list",
                    _ => {
                        self.input_controller
                            .push_sys_err("Unhandled fetch item".to_owned());
                        return HandleCommandStatus::Continue;
                    }
                };

                if let Err(e) = self
                    .outgoing_tx
                    .send(
                        FetchReq {
                            item: item_str.to_owned(),
                        }
                        .as_json_string(),
                    )
                    .await
                {
                    self.input_controller
                        .push_sys_err(format!("Channel send failed, try again: '{}'", e));
                    return HandleCommandStatus::Continue;
                }

                // block til Login response
                let fetch_res = util::consume_til::<FetchRes>(self.incoming_tx.subscribe()).await;
                match fetch_res.item.as_str() {
                    "list" => match fetch_res.result {
                        Ok(v) => self
                            .input_controller
                            .push_sys_msg(serde_json::to_string_pretty(&v).unwrap()),
                        Err(e) => self.input_controller.push_sys_err(e),
                    },
                    unknown => self
                        .input_controller
                        .push_sys_err(format!("unknown item: '{}'", unknown)),
                };
            }
            Ok(Command::Goto(channel_name)) => {
                _ = self
                    .outgoing_tx
                    .send(GotoReq { channel_name }.as_json_string())
                    .await;
                match util::consume_til::<GotoRes>(self.incoming_tx.subscribe())
                    .await
                    .result
                {
                    Ok(name) => {
                        // goto succeeded, change channel
                        self.state.channel = name.clone();
                        self.input_controller
                            .push_sys_msg(format!("succeeded to join channel: '{}'", name));
                    }
                    Err(e) => self
                        .input_controller
                        .push_sys_err(format!("failed to join channel: '{}'", e)),
                }
            }
            Ok(Command::Exit) => {
                self.input_controller
                    .push_sys_msg(" >> See You Soon << ".to_owned());
                _ = self.outgoing_tx.send(Exit {}.as_json_string()).await;
                return HandleCommandStatus::Exit;
            }
            // Not a command
            Err(ParseCommandError::UnknownCommand(cmd)) => self
                .input_controller
                .push_sys_err(format!("Unknown command: {}", cmd)),
            Err(e) => self.input_controller.push_sys_err(format!("{:?}", e)),
        }
        HandleCommandStatus::Continue
    }
}
