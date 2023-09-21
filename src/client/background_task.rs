use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::{broadcast, mpsc},
};

use crate::{client::util, packet::*};

/// receive formatted packets from `rd` and enqueue them to `incoming_tx` channel
pub async fn produce_incomings(
    mut rd: ReadHalf<TcpStream>,
    incoming_tx: broadcast::Sender<String>,
) {
    loop {
        // Size header
        let size_msg = match rd.read_u32().await {
            Ok(0) | Err(_) => panic!("[#System] EOF"),
            Ok(size) => size,
        };

        // Message body
        let mut buf = vec![0; size_msg as usize];
        let n = match rd.read_exact(buf.as_mut_slice()).await {
            Ok(0) | Err(_) => panic!("[#System] EOF"),
            Ok(size) => size,
        };

        let msg_str = String::from_utf8(buf[0..n].to_vec()).unwrap();
        _ = incoming_tx.send(msg_str);
    }
}

/// handle message packets
pub async fn print_message_packets(mut incoming_rx: broadcast::Receiver<String>) {
    loop {
        let msg_str = match incoming_rx.recv().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        match serde_json::from_str::<Message>(msg_str.as_str()) {
            Ok(msg) if msg.is_system => println!("[#System] {}", msg.msg),
            Ok(msg) => println!("{}{}: {}", util::get_mark(&msg.id), msg.id, msg.msg),
            _ => (),
        }
    }
}

pub async fn consume_outgoings(
    mut write_stream: WriteHalf<TcpStream>,
    mut outgoing_rx: mpsc::Receiver<String>,
) {
    while let Some(msg) = outgoing_rx.recv().await {
        _ = write_stream.write_all(msg.as_bytes()).await;
    }
}
