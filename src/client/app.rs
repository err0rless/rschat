use std::str::FromStr;

use tokio::sync::{broadcast, mpsc};

use super::{
    command::*,
    input_controller::*,
    message_channel::MessageChannel,
    popup::{self, login::LoginPopupManager, register::RegisterPopupManager},
    session, util,
};
use crate::{crypto::hash, db, packet::*};

#[derive(PartialEq)]
pub enum HandleCommandStatus {
    // Requested to exit program
    Exit,

    // Continue to handle
    Continue,
}

pub enum CommandAction {
    Login,
    Register,
}

/// App holds the state of the application
pub struct App {
    pub main_input: InputController,
    pub messages: MessageChannel,
    pub outgoing_tx: mpsc::Sender<String>,
    pub incoming_tx: broadcast::Sender<String>,
    pub state: session::State,
    pub popup: Option<Box<dyn popup::PopupManager>>,
}

impl App {
    pub fn new(
        outgoing_tx: mpsc::Sender<String>,
        incoming_tx: broadcast::Sender<String>,
        state: session::State,
    ) -> Self {
        Self {
            main_input: InputController::default(),
            messages: MessageChannel::default(),
            outgoing_tx,
            incoming_tx,
            state,
            popup: None,
        }
    }

    /// Send message to the outgoing channel
    pub async fn send_message(&self) {
        let msg_bytes = Message {
            id: self.state.id.clone(),
            msg: self.main_input.buf.clone(),
            is_system: false,
        }
        .as_json_string();
        _ = self.outgoing_tx.send(msg_bytes).await;
    }

    pub async fn run_action(&mut self, action: &CommandAction, args: Option<serde_json::Value>) {
        match action {
            CommandAction::Login => {
                let args = args.unwrap();
                let id = args["id"].as_str().unwrap();
                let password = args["password"].as_str().unwrap();
                self.login(id, password).await;
            }
            CommandAction::Register => {
                let args = args.unwrap();
                self.register(
                    args["id"].as_str().unwrap(),
                    args["password"].as_str().unwrap(),
                    args["bio"].as_str(),
                    args["location"].as_str(),
                )
                .await;
            }
        };
    }

    pub async fn login(&mut self, id: &str, password: &str) {
        if !self.state.is_guest {
            self.messages
                .push_sys_err("You are already logged in".to_owned());
            return;
        }

        let login_info = db::user::Login {
            guest: false,
            id: Some(id.to_owned()),
            password: Some(hash::sha256_password(password)),
        };

        // id backup
        let id_clone = login_info.id.clone().unwrap();
        if let Err(e) = self
            .outgoing_tx
            .send(LoginReq { login_info }.as_json_string())
            .await
        {
            self.messages
                .push_sys_err(format!("Channel send failed, try again: '{}'", e));
            return;
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
                self.messages.push_sys_msg("Success!".to_owned());
            }
            Err(s) => self.messages.push_sys_err(format!("Failure: '{}'", s)),
        };
    }

    pub async fn register(
        &mut self,
        id: &str,
        password: &str,
        bio: Option<&str>,
        location: Option<&str>,
    ) {
        if id.is_empty() || password.is_empty() {
            self.messages
                .push_sys_err("ID or Password is empty".to_owned());
            return;
        }

        let user = db::user::User {
            id: id.to_owned(),
            password: hash::sha256_password(password),
            bio: bio.map(String::from),
            location: location.map(String::from),
        };

        let register_req = RegisterReq { user }.as_json_string();
        if let Err(e) = self.outgoing_tx.send(register_req).await {
            self.messages
                .push_sys_err(format!("Channel send failed, retry later: {}", e));
        }

        // block til Register response
        self.messages.push_sys_msg(
            match util::consume_til::<RegisterRes>(self.incoming_tx.subscribe())
                .await
                .result
            {
                Ok(_) => "Success!".to_owned(),
                Err(s) => format!("Failure: {}", s),
            },
        );
    }

    pub async fn handle_command(&mut self) -> HandleCommandStatus {
        match Command::from_str(&self.main_input.buf) {
            Ok(Command::Help) => Command::help(),
            Ok(Command::Get(item)) => match &item[..] {
                "info" | "name" => self
                    .messages
                    .push_sys_msg(format!("Your ID: '{}'", self.state.id)),
                _ => self
                    .messages
                    .push_sys_err(format!("Unknown item for 'get' command: '{}'", item)),
            },
            Ok(Command::Register) => {
                self.main_input.normal_mode();
                self.popup = Some(Box::new(RegisterPopupManager::new()));
            }
            Ok(Command::Login()) => {
                self.main_input.normal_mode();
                self.popup = Some(Box::new(LoginPopupManager::new()));
            }
            Ok(Command::Fetch(fetch)) => {
                let item_str = match fetch {
                    Fetch::UserList => "list",
                    _ => {
                        self.messages
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
                    self.messages
                        .push_sys_err(format!("Channel send failed, try again: '{}'", e));
                    return HandleCommandStatus::Continue;
                }

                // block til Login response
                let fetch_res = util::consume_til::<FetchRes>(self.incoming_tx.subscribe()).await;
                match fetch_res.item.as_str() {
                    "list" => match fetch_res.result {
                        Ok(v) => self
                            .messages
                            .push_sys_msg(serde_json::to_string_pretty(&v).unwrap()),
                        Err(e) => self.messages.push_sys_err(e),
                    },
                    unknown => self
                        .messages
                        .push_sys_err(format!("unknown item: '{}'", unknown)),
                }
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
                        self.messages.push_sys_msg(format!(
                            "You've succesfully switched to the channel: '{}'",
                            &name
                        ));
                        self.state.channel = name;
                    }
                    Err(e) => self
                        .messages
                        .push_sys_err(format!("failed to join channel: '{}'", e)),
                }
            }
            Ok(Command::Exit) => {
                _ = self.outgoing_tx.send(Exit {}.as_json_string()).await;
                return HandleCommandStatus::Exit;
            }
            // Not a command
            Err(ParseCommandError::UnknownCommand(cmd)) => self
                .messages
                .push_sys_err(format!("Unknown command: {}", cmd)),
            Err(e) => self.messages.push_sys_err(format!("{:?}", e)),
        }
        HandleCommandStatus::Continue
    }
}
