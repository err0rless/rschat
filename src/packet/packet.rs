use serde::{Deserialize, Serialize};
use serde_json::Value;

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

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct Join {
    pub id: String,
}

impl AsJson for Join {}

// Client <- Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub struct JoinResult {
    pub id: String,
    pub result: bool,
    pub msg: String,
}

impl AsJson for JoinResult {}

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
    Join(Join),
    JoinResult(JoinResult),
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
                Some("Join") => {
                    let j: Join = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::Join(j))
                }
                Some("Message") => {
                    let m: Message = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::Message(m))
                }
                Some("JoinResult") => {
                    let r: JoinResult = serde_json::from_value(json_value).unwrap();
                    Some(PacketType::JoinResult(r))
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
