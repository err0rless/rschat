pub fn get_mark(id: &str) -> char {
    match id {
        s if s.starts_with("guest_") => '%',
        s if s.starts_with("root") => '#',
        _ => '@',
    }
}

/// Consumes broadcast channel until encounter the packet type `P`
pub async fn consume_til<P>(mut incoming_rx: tokio::sync::broadcast::Receiver<String>) -> P
where
    P: serde::de::DeserializeOwned,
{
    loop {
        if let Ok(msg) = incoming_rx.recv().await {
            let j: serde_json::Value = serde_json::from_str(msg.as_str()).unwrap();
            if let Ok(res) = serde_json::from_value::<P>(j) {
                return res;
            }
        }
    }
}
