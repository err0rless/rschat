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

// Client -> Server -> Other clients
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Message {
    pub id: String,
    pub msg: String,
    pub is_system: bool,
}

impl AsJson for Message {}

// request format for registration
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct RegisterReq {
    pub user: db::user::User,
}

impl AsJson for RegisterReq {}

// response format for registration
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct RegisterRes {
    pub result: Result<(), String>,
}

impl AsJson for RegisterRes {}

// request format for login
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct LoginReq {
    pub login_info: db::user::Login,
}

impl AsJson for LoginReq {}

// response format for login
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct LoginRes {
    pub result: Result<String /* id */, String>,
}

impl AsJson for LoginRes {}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct GuestJoinReq {}

impl AsJson for GuestJoinReq {}

// Client <- Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct GuestJoinRes {
    pub id: Result<String, String>,
}

impl AsJson for GuestJoinRes {}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Connected {}

impl AsJson for Connected {}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Exit {}

impl AsJson for Exit {}

#[derive(Clone)]
pub enum PacketType {
    GuestJoinReq(GuestJoinReq),
    GuestJoinRes(GuestJoinRes),
    RegisterReq(RegisterReq),
    RegisterRes(RegisterRes),
    LoginReq(LoginReq),
    LoginRes(LoginRes),
    Connected(Connected),
    Message(Message),
    Exit(Exit),
}

impl PacketType {
    pub fn from_str(data: &str) -> Option<Self> {
        let json_value: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => return None,
        };

        let map = match json_value.as_object() {
            Some(m) => m,
            None => return None,
        };

        if let Some(packet_type) = map.get("type") {
            match packet_type.as_str() {
                Some("GuestJoinReq") => {
                    let j: GuestJoinReq = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::GuestJoinReq(j))
                }
                Some("GuestJoinRes") => {
                    let r: GuestJoinRes = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::GuestJoinRes(r))
                }
                Some("RegisterReq") => {
                    let r: RegisterReq = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::RegisterReq(r))
                }
                Some("RegisterRes") => {
                    let r: RegisterRes = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::RegisterRes(r))
                }
                Some("LoginReq") => {
                    let r: LoginReq = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::LoginReq(r))
                }
                Some("LoginRes") => {
                    let r: LoginRes = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::LoginRes(r))
                }
                Some("Message") => {
                    let m: Message = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::Message(m))
                }
                Some("Connected") => Some(PacketType::Connected(Connected {})),
                Some("Exit") => Some(PacketType::Exit(Exit {})),
                Some(unknown_type) => {
                    println!("[!] Unknown packet type: {}", unknown_type);
                    None
                }
                None => None,
            }
        } else {
            None
        }
    }
}
