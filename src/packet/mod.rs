use std::str::FromStr;

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

#[derive(Debug, PartialEq, Eq)]
pub struct ParsePacketTypeError;

impl From<()> for ParsePacketTypeError {
    fn from(_: ()) -> Self {
        ParsePacketTypeError {}
    }
}

impl FromStr for PacketType {
    type Err = ParsePacketTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Ok(json_value): Result<Value, _> = serde_json::from_str(s) else {
            return Err(ParsePacketTypeError);
        };

        macro_rules! packet_from_str {
            ($packet:ident) => {{
                let r: $packet = serde_json::from_value(json_value).unwrap();
                Ok(PacketType::$packet(r))
            }};
        }

        let packet_type = json_value.as_object().ok_or(())?.get("type").ok_or(())?;
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
            Some("Connected") => Ok(PacketType::Connected(Connected {})),
            Some("Exit") => Ok(PacketType::Exit(Exit {})),
            Some(unknown_type) => {
                println!("[!] Unknown packet type: {}", unknown_type);
                Err(ParsePacketTypeError)
            }
            None => Err(ParsePacketTypeError),
        }
    }
}
