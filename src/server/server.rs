use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

use crate::packet::packet::*;
use crate::server::session;

fn check_join_request(state: &session::State, join: &Join) -> Result<(), String> {
    if state.num_user >= session::NUM_MAX_USER {
        return Err(String::from("Server is full"));
    } else if state.names.contains(&join.id) {
        return Err(String::from("Duplicate ID"));
    } else if join.id.len() < 4 || join.id.len() > 12 {
        return Err(String::from("Length of ID should be: 3 < ID < 13"));
    }
    Ok(())
}

async fn broadcast_channel_handler(
    mut msg_rx: broadcast::Receiver<PacketType>,
    mut wr: WriteHalf<TcpStream>,
    id: Arc<Mutex<String>>,
) {
    let connected = AtomicBool::new(false);
    loop {
        match msg_rx.recv().await {
            Ok(PacketType::JoinResult(res)) => {
                if let Ok(lock) = id.lock() {
                    if lock.as_str() != res.id {
                        continue;
                    }
                }
                _ = wr.write_all(&res.as_json_bytes()).await;
            }
            Ok(PacketType::Message(msg)) => {
                // Client hasn't connected successfully yet
                if !connected.load(Ordering::Relaxed) {
                    continue;
                }

                // Skip message from the current client handler
                if let Ok(lock) = id.lock() {
                    if lock.as_str() == msg.id {
                        continue;
                    }
                }

                // Write message to the stream
                if let Ok(msg_json) = serde_json::to_value(msg) {
                    _ = wr.write_all(msg_json.to_string().as_bytes()).await;
                }
            }
            Ok(PacketType::Connected(_)) => {
                connected.store(true, Ordering::Relaxed);
            }
            _ => continue,
        }
    }
}

async fn client_handler(
    stream: TcpStream,
    msg_tx: broadcast::Sender<PacketType>,
    state: Arc<Mutex<session::State>>,
) {
    let (mut rd, wr) = tokio::io::split(stream);

    // Identifier container
    let id = Arc::new(Mutex::new(String::new()));

    // Subscribe the broadcast channel
    let msg_rx = msg_tx.subscribe();

    // broadcast channel handler
    tokio::task::spawn(broadcast_channel_handler(msg_rx, wr, id.clone()));

    let mut buf = [0; 1024];
    loop {
        // read data from client
        let n = match rd.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => {
                return;
            }
        };

        let msg_str = if let Ok(s) = std::str::from_utf8(&buf[0..n]) {
            s
        } else {
            continue;
        };
        match PacketType::from_str(msg_str) {
            // Received request to join, check to see if the client is good to join and send
            // result back
            Some(PacketType::Join(join)) => {
                let join_res: JoinResult = {
                    let mut s = state.lock().unwrap();
                    let acceptable = check_join_request(&s, &join);

                    // add new user to Session
                    if acceptable.is_ok() {
                        match id.lock() {
                            // Set id for current client handler
                            Ok(mut lock) if lock.is_empty() => lock.push_str(join.id.as_str()),
                            // 1. mutex lock failed
                            // 2. client is trying to join more than once
                            _ => continue,
                        }

                        s.names.insert(join.id.clone());
                        s.num_user += 1;
                    }

                    JoinResult {
                        id: join.id,
                        result: acceptable.is_ok(),
                        msg: acceptable.err().unwrap_or(String::from("success")),
                    }
                };
                // notify client that it's ok to join
                _ = msg_tx.send(PacketType::JoinResult(join_res.clone()));

                match id.lock() {
                    // Join request has been aceepted
                    Ok(lock) if join_res.result => {
                        _ = msg_tx.send(PacketType::Connected(Connected {}));
                        _ = msg_tx.send(PacketType::Message(Message {
                            id: lock.clone(),
                            msg: format!("@{} has joined", lock),
                            is_system: true,
                        }));
                    }
                    _ => (),
                }
            }
            // Received request to broadcast message
            Some(PacketType::Message(msg)) => {
                // Send message to the channel for broadcasting to connected clients
                _ = msg_tx.send(PacketType::Message(msg));
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

pub async fn server_main(port: String) -> Result<(), Box<dyn std::error::Error>> {
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(l) => l,
        Err(e) => panic!("{}", e),
    };

    // Channel for broadcasting messages to every connected client
    let (msg_tx, _) = broadcast::channel::<PacketType>(32);

    // Session state
    let state = Arc::new(Mutex::new(session::State {
        names: std::collections::HashSet::new(),
        num_user: 0,
    }));

    println!("[RsChat Sever] Listening on port {}...", port);
    while let Ok(s) = listener.accept().await {
        println!("New connection from: {:?}", s.0);
        tokio::spawn(client_handler(s.0, msg_tx.clone(), state.clone()));
    }
    Ok(())
}
