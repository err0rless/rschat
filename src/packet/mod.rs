use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db;

pub trait AsJson {
    fn as_json_string(&self) -> String
    where
        Self: Serialize,
    {
        serde_json::to_string(&self).unwrap()
    }

    fn as_json_bytes(&self) -> Vec<u8>
    where
        Self: Serialize,
    {
        serde_json::to_vec(&self).unwrap()
    }
}

// macro for packet declarations
macro_rules! packet_declarations {
    ($($vis:vis struct $name:ident $body:tt)*) => {
        $(
            #[derive(Serialize, Deserialize, Debug, Clone)]
            #[serde(tag = "type")]
            $vis struct $name $body

            impl AsJson for $name {}
        )*
    }
}

packet_declarations! {

pub struct Message {
    pub id: String,
    pub msg: String,
    pub is_system: bool,
}

pub struct RegisterReq {
    pub user: db::user::User,
}

pub struct RegisterRes {
    pub result: Result<(), String>,
}

pub struct LoginReq {
    pub login_info: db::user::Login,
}

pub struct LoginRes {
    pub result: Result<String /* id */, String>,
}

pub struct FetchReq {
    pub item: String,
}

pub struct FetchRes {
    pub item: String,
    pub result: Result<serde_json::Value, String>,
}

pub struct GotoReq {
    pub channel_name: String,
}

pub struct GotoRes {
    pub result: Result<String, String>,
}

// notify that a new client has connected
pub struct Connected {}

// notify that a client has disconnected
pub struct Exit {}

}

impl Message {
    pub fn connection(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            msg: format!("'{}' has joined", id),
            is_system: true,
        }
    }

    pub fn disconnection(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            msg: format!("'{}' has left", id),
            is_system: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum PacketType {
    RegisterReq(RegisterReq),
    RegisterRes(RegisterRes),
    LoginReq(LoginReq),
    LoginRes(LoginRes),
    FetchReq(FetchReq),
    FetchRes(FetchRes),
    GotoReq(GotoReq),
    GotoRes(GotoRes),
    Connected(Connected),
    Message(Message),
    Exit(Exit),
}

impl PacketType {
    pub fn from_str(data: &str) -> Option<Self> {
        let json_value: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return None,
        };

        let map = match json_value.as_object() {
            Some(m) => m,
            None => return None,
        };

        let packet_type = match map.get("type") {
            Some(pt) => pt,
            None => return None,
        };

        macro_rules! packet_from_str {
            ($packet:ident) => {{
                let r: $packet = serde_json::from_value(json_value).unwrap();
                Some(PacketType::$packet(r))
            }};
        }

        match packet_type.as_str() {
            Some("RegisterReq") => packet_from_str!(RegisterReq),
            Some("RegisterRes") => packet_from_str!(RegisterRes),
            Some("LoginReq") => packet_from_str!(LoginReq),
            Some("LoginRes") => packet_from_str!(LoginRes),
            Some("FetchReq") => packet_from_str!(FetchReq),
            Some("FetchRes") => packet_from_str!(FetchRes),
            Some("GotoReq") => packet_from_str!(GotoReq),
            Some("GotoRes") => packet_from_str!(GotoRes),
            Some("Message") => packet_from_str!(Message),
            Some("Connected") => Some(PacketType::Connected(Connected {})),
            Some("Exit") => Some(PacketType::Exit(Exit {})),
            Some(unknown_type) => {
                println!("[!] Unknown packet type: {}", unknown_type);
                None
            }
            None => None,
        }
    }
}
