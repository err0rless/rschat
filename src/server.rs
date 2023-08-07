use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

mod packet;
use packet::packet::*;

mod session;

fn check_join_request(state: &session::State, join: &Join) -> Result<(), String> {
    if state.num_user >= session::NUM_MAX_USER {
        return Err(String::from("Server is full"));
    } else if state.names.contains(&join.id) {
        return Err(String::from("Duplicate ID"));
    }
    Ok(())
}

async fn channel_handler(
    mut msg_rx: broadcast::Receiver<PacketType>,
    mut packet_rx: mpsc::Receiver<PacketType>,
    mut wr: WriteHalf<TcpStream>,
    id_for_channel: Arc<Mutex<String>>,
) {
    let connected = AtomicBool::new(false);
    loop {
        tokio::select! {
            // Handling the message channel
            Ok(packet_type) = msg_rx.recv() => {
                match packet_type {
                    PacketType::Message(msg) => {
                        // Client hasn't connected successfully yet
                        if !connected.load(Ordering::Relaxed) { continue; }

                        // Skip message from the current client handler
                        if let Ok(lock) = id_for_channel.lock() {
                            if lock.as_str() == msg.id {
                                continue
                            }
                        }

                        // Write message to the stream
                        if let Ok(msg_json) = serde_json::to_value(msg) {
                            _ = wr.write_all(msg_json.to_string().as_bytes()).await;
                        }
                    },
                    PacketType::Connected(_) => {
                        connected.store(true, Ordering::Relaxed);
                    }
                    _ => (),
                }
            },
            // Handling the packet channel that takes any type of packet
            Some(PacketType::JoinResult(res)) = packet_rx.recv() => {
                _ = wr.write_all(&res.as_json_bytes()).await;
            }
        }
    }
}

async fn client_handler(
    stream: TcpStream,
    msg_tx: broadcast::Sender<PacketType>,
    state: Arc<Mutex<session::State>>,
) {
    let (mut rd, wr) = tokio::io::split(stream);

    // New packet channel only for current client handler
    let (packet_tx, packet_rx) = mpsc::channel::<PacketType>(8);

    // Identifier container
    let id = Arc::new(Mutex::new(String::new()));

    // Subscribe the broadcast channel
    let msg_rx = msg_tx.subscribe();

    // Channels handler
    //  - msg_rx: Message channel (broadcast, every client handler should have this)
    //  - packet_rx: Packet channel (mspc, only for current client handler
    tokio::task::spawn(channel_handler(msg_rx, packet_rx, wr, id.clone()));

    let mut buf = [0; 1024];
    loop {
        // read data from client
        let n = match rd.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) => {
                println!("failed to read from socket; err = {:?}", e);
                return;
            }
        };

        let msg_str = std::str::from_utf8(&buf[0..n]).unwrap();
        match PacketType::from_str(msg_str) {
            // Received request to join from the client, check to see if the client is good to join
            // and write result back to the client
            Some(PacketType::Join(join)) => {
                let join_res: JoinResult = {
                    let mut s = state.lock().unwrap();
                    let acceptable = check_join_request(&s, &join);

                    // add new user to Session
                    if acceptable.is_ok() {
                        s.names.insert(join.id.clone());
                        s.num_user += 1;

                        // Set id for current client handler
                        if let Ok(mut lock) = id.lock() {
                            lock.push_str(join.id.as_str());
                        }
                    }
                    JoinResult {
                        result: acceptable.is_ok(),
                        msg: acceptable.err().unwrap_or(String::from("success")),
                    }
                };
                // notify client that it's ok to join
                _ = packet_tx.send(PacketType::JoinResult(join_res)).await;
            }
            // Received request to broadcast message from client
            Some(PacketType::Message(msg)) => {
                // Send message to the channel for broadcasting to connected clients
                _ = msg_tx.send(PacketType::Message(msg));
            }
            // Received notification that the client was successfully connected to the server
            // Now this client can receive the messages from other clients
            Some(PacketType::Connected(con)) => {
                _ = msg_tx.send(PacketType::Connected(con));

                if let Ok(lock) = id.lock() {
                    // notify other clients that new client has joined
                    let con_notification = Message {
                        id: lock.clone(),
                        msg: format!("@{} has joined the server", lock),
                        is_system: true,
                    };
                    _ = msg_tx.send(PacketType::Message(con_notification));
                }
            }
            // Received exit notification from client, remove the client from current session
            Some(PacketType::Exit(_)) => {
                let mut s = state.lock().unwrap();
                s.num_user -= 1;

                if let Ok(lock) = id.lock() {
                    // remove user from Session
                    s.names.remove(lock.as_str());

                    // notify other clients that this client has left
                    let exit_notification = Message {
                        id: lock.clone(),
                        msg: format!("@{} has left the server", lock),
                        is_system: true,
                    };
                    _ = msg_tx.send(PacketType::Message(exit_notification));
                }
                return;
            }
            None => {
                println!("[!] Failed to parse packet from: '{}'", msg_str);
            }
            _ => {}
        };
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;

    // Channel for broadcasting messages to every connected client
    let (msg_tx, _) = broadcast::channel::<PacketType>(32);

    // Session state
    let state = Arc::new(Mutex::new(session::State {
        names: std::collections::HashSet::new(),
        num_user: 0,
    }));

    println!("[RsChat Sever] Listening on port 8080...");
    loop {
        let socket = match listener.accept().await {
            Ok(s) => {
                println!("[#System] new connection from {:?}", s.0);
                s.0
            }
            Err(_) => continue,
        };
        tokio::spawn(client_handler(socket, msg_tx.clone(), state.clone()));
    }
}
