use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db;

// Client -> Server -> Other clients
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Message {
    pub id: String,
    pub msg: String,
    pub is_system: bool,
}

impl Message {
    pub fn connection(id: &String) -> Self {
        Self {
            id: id.clone(),
            msg: format!("'{}' has joined", id),
            is_system: true,
        }
    }

    pub fn disconnection(id: &String) -> Self {
        Self {
            id: id.clone(),
            msg: format!("'{}' has left", id),
            is_system: true,
        }
    }
}

// request format for registration
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct RegisterReq {
    pub user: db::user::User,
}

// response format for registration
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct RegisterRes {
    pub result: Result<(), String>,
}

// request format for login
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct LoginReq {
    pub login_info: db::user::Login,
}

// response format for login
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct LoginRes {
    pub result: Result<String /* id */, String>,
}

// Fetch information request format
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct FetchReq {
    pub item: String,
}

// Fetch information request format
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct FetchRes {
    pub item: String,
    pub result: Result<serde_json::Value, String>,
}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Connected {}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Exit {}

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

impl AsJson for serde_json::Value {}
impl AsJson for Message {}
impl AsJson for RegisterReq {}
impl AsJson for RegisterRes {}
impl AsJson for LoginReq {}
impl AsJson for LoginRes {}
impl AsJson for FetchReq {}
impl AsJson for FetchRes {}
impl AsJson for Connected {}
impl AsJson for Exit {}

#[derive(Clone)]
pub enum PacketType {
    RegisterReq(RegisterReq),
    RegisterRes(RegisterRes),
    LoginReq(LoginReq),
    LoginRes(LoginRes),
    FetchReq(FetchReq),
    FetchRes(FetchRes),
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

        if let Some(packet_type) = map.get("type") {
            match packet_type.as_str() {
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
                Some("FetchReq") => {
                    let fetch_req: FetchReq = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::FetchReq(fetch_req))
                }
                Some("FetchRes") => {
                    let fetch_res: FetchRes = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::FetchRes(fetch_res))
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
