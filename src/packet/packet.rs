use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub trait IntoPacket {
    fn as_json(&self) -> Value;
    fn into_json(self) -> Value;

    fn as_json_bytes(&self) -> Vec<u8> {
        self.as_json().to_string().as_bytes().to_vec()
    }

    fn as_json_string(&self) -> String {
        self.as_json().to_string()
    }
}

// Client -> Server -> Other clients
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub id: String,
    pub msg: String,
    pub is_system: bool,
}

impl IntoPacket for Message {
    fn as_json(&self) -> Value {
        json!({
            "id": self.id.clone(),
            "msg": self.msg.clone(),
            "is_system": self.is_system,
            "type": "msg",
        })
    }

    fn into_json(self) -> Value {
        json!({
            "id": self.id,
            "msg": self.msg,
            "is_system": self.is_system,
            "type": "msg",
        })
    }
}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Join {
    pub id: String,
}

impl IntoPacket for Join {
    fn as_json(&self) -> Value {
        json!({
            "id": self.id.clone(),
            "type": "join",
        })
    }

    fn into_json(self) -> Value {
        json!({
            "id": self.id,
            "type": "join",
        })
    }
}

// Client <- Server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JoinResult {
    pub id: String,
    pub result: bool,
    pub msg: String,
}

impl IntoPacket for JoinResult {
    fn as_json(&self) -> Value {
        json!({
            "id": self.id,
            "result": self.result,
            "msg": self.msg,
            "type": "join_result",
        })
    }

    fn into_json(self) -> Value {
        json!({
            "id": self.id,
            "result": self.result,
            "msg": self.msg,
            "type": "join_result",
        })
    }
}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Connected {}

impl IntoPacket for Connected {
    fn as_json(&self) -> Value {
        json!({ "type": "connected", })
    }

    fn into_json(self) -> Value {
        json!({ "type": "connected", })
    }
}

// Client -> Server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Exit {}

impl IntoPacket for Exit {
    fn as_json(&self) -> Value {
        json!({ "type": "exit", })
    }

    fn into_json(self) -> Value {
        json!({ "type": "exit", })
    }
}

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
                Some("join") => {
                    let id = map.get("id").unwrap().as_str().unwrap();
                    Some(PacketType::Join(Join {
                        id: String::from(id),
                    }))
                }
                Some("msg") => {
                    let id = map.get("id").unwrap().as_str().unwrap();
                    let msg = map.get("msg").unwrap().as_str().unwrap();
                    let is_system = map.get("is_system").unwrap().as_bool().unwrap();
                    Some(PacketType::Message(Message {
                        id: String::from(id),
                        msg: String::from(msg),
                        is_system,
                    }))
                }
                Some("join_result") => {
                    let id = map.get("id").unwrap().as_str().unwrap();
                    let result = map.get("result").unwrap().as_bool().unwrap();
                    let msg = map.get("msg").unwrap().as_str().unwrap();
                    Some(PacketType::JoinResult(JoinResult {
                        id: String::from(id),
                        result,
                        msg: String::from(msg),
                    }))
                }
                Some("exit") => Some(PacketType::Exit(Exit {})),
                Some("connected") => Some(PacketType::Connected(Connected {})),
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
