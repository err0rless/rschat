use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

use rand::prelude::*;

use crate::packet::packet::*;
use crate::server::session;

/// Write `packet` to the TCP stream with size header
async fn send_sized_msg<P>(wr: &mut WriteHalf<TcpStream>, packet: P)
where
    P: AsJson + serde::Serialize,
{
    // super simple message protocol [Size: u32][Message: String]
    let packet_bytes = packet.as_json_bytes();
    _ = wr.write_u32(packet_bytes.len() as u32).await;
    _ = wr.write_all(&packet_bytes).await;
}

async fn channel_consumer(
    mut msg_rx: broadcast::Receiver<PacketType>,
    mut res_rx: mpsc::Receiver<PacketType>,
    mut wr: WriteHalf<TcpStream>,
    id: Arc<Mutex<String>>,
) {
    let connected = AtomicBool::new(false);
    loop {
        tokio::select! {
            // Handling broadcasting packets
            //
            // Packets queued on this channel can be sent to any subscriber
            msg = msg_rx.recv() => match msg {
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
                        send_sized_msg(&mut wr, msg_json).await;
                    }
                }
                Ok(PacketType::Connected(_)) => {
                    connected.store(true, Ordering::Relaxed);
                }
                _ => continue,
            },
            // Handling packets that will be sent to current client
            //
            // Any one-on-one comminucation between client and server such as request/response should
            // be sent to this channel
            res = res_rx.recv() => match res {
                Some(PacketType::RegisterRes(r)) => {
                    send_sized_msg(&mut wr, r).await;
                }
                Some(PacketType::LoginRes(mut r)) => {
                    // Login was successful, update the id
                    if let Ok(mut lock) = id.lock() {
                        if let Ok(login_id) = &r.result {
                            connected.store(true, Ordering::Relaxed);
                            *lock = login_id.clone();
                        }
                    } else if r.result.is_ok() {
                        // somehow failed to lock the id
                        r.result = Err(format!("failed to login"));
                    }
                    send_sized_msg(&mut wr, r).await;
                }
                _ => continue,
            },
        }
    }
}

// Handler for each connection
async fn session_task(
    stream: TcpStream,
    msg_tx: broadcast::Sender<PacketType>,
    state: Arc<Mutex<session::State>>,
    sqlconn: Arc<Mutex<rusqlite::Connection>>,
) {
    // Split into two unidirectional stream
    let (mut rd, wr) = tokio::io::split(stream);

    // Thread-safe id container
    let id = Arc::new(Mutex::new(String::new()));

    // Subscribe the broadcast channel
    let msg_rx = msg_tx.subscribe();

    // Channel for sending response back to client, or any type of packet that needs to be sent
    // to only current client
    let (res_tx, res_rx) = mpsc::channel::<PacketType>(32);

    // Channel consumer: consumes the messages from the channels and handle them
    //  - msg_rx: broadcast channel
    //  - res_rx: mpsc channel
    tokio::task::spawn(channel_consumer(msg_rx, res_rx, wr, id.clone()));

    let mut buf = [0; 1024];
    loop {
        // read data from client
        let n = match rd.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => return,
        };

        let msg_str = if let Ok(s) = std::str::from_utf8(&buf[0..n]) {
            s
        } else {
            continue;
        };
        match PacketType::from_str(msg_str) {
            // Received a request to create a new account
            Some(PacketType::RegisterReq(req)) => {
                let res = RegisterRes {
                    result: req.user.insert(sqlconn.clone()),
                };
                _ = res_tx.send(PacketType::RegisterRes(res)).await;
            }
            // Received a request to login
            Some(PacketType::LoginReq(req)) => {
                let res = LoginRes {
                    result: 'outer: {
                        let mut lock = if let Ok(lock) = state.lock() {
                            lock
                        } else {
                            break 'outer Err(format!("unknown error!"));
                        };

                        if req.login_info.guest {
                            // Guets Login
                            if lock.num_guest >= session::NUM_MAX_GUEST {
                                break 'outer Err(format!("too many guests"));
                            }

                            // Generate a random guest name
                            let mut rng = rand::thread_rng();
                            let guest_id = loop {
                                let random_id = format!("guest_{}", rng.gen::<u16>());

                                // duplicate check
                                if !lock.names.contains(&random_id) {
                                    break random_id;
                                }
                            };

                            // state modification
                            lock.names.insert(guest_id.clone());
                            lock.num_guest += 1;

                            Ok(guest_id.clone())
                        } else {
                            // Account Login
                            if lock.num_user >= session::NUM_MAX_USER {
                                break 'outer Err(format!("too many users"));
                            }

                            // validation of inputs was done before this packet reached here, but somehow it's broken
                            if req.login_info.id.is_none() || req.login_info.password.is_none() {
                                break 'outer Err(format!("broken login packet"));
                            }

                            let res = req.login_info.login(sqlconn.clone());
                            if res.is_ok() {
                                lock.num_guest -= 1; // every connection is a guest at first
                                lock.num_user += 1;
                                lock.names.insert(req.login_info.id.clone().unwrap());

                                _ = msg_tx.send(PacketType::Message(Message::connection(
                                    &req.login_info.id.unwrap(),
                                )));
                            }
                            res
                        }
                    },
                };
                _ = res_tx.send(PacketType::LoginRes(res)).await;
            }
            // Received a request to broadcast message
            Some(PacketType::Message(msg)) => {
                // Send message to the channel for broadcasting to connected clients
                _ = msg_tx.send(PacketType::Message(msg));
            }
            // Received exit notification from client, remove the client from current session
            Some(PacketType::Exit(_)) => {
                let mut s = state.lock().unwrap();
                if let Ok(lock) = id.lock() {
                    // leaving user is a guest
                    if lock.starts_with("guest_") {
                        s.num_guest -= 1;
                    } else {
                        s.num_user -= 1;
                    }

                    // remove user from Session
                    s.names.remove(lock.as_str());

                    // disconnection broadcasting
                    _ = msg_tx.send(PacketType::Message(Message::disconnection(&lock.clone())));
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

pub async fn run_server(port: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("[RsChat Sever] Bining on port {}...", port);
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(l) => l,
        Err(e) => panic!("{}", e),
    };

    // Channel for broadcasting messages to subscribers
    let (msg_tx, _) = broadcast::channel::<PacketType>(32);

    // Session state
    let state = Arc::new(Mutex::new(session::State {
        names: std::collections::HashSet::new(),
        num_user: 0,
        num_guest: 0,
    }));

    // In-memory sqlite instance
    let sqlconn = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory()?));

    // Create essential tables / columns
    if let Ok(lock) = sqlconn.lock() {
        lock.execute_batch(
            "BEGIN;
            CREATE TABLE user (
                id          TEXT PRIMARY KEY,
                password    TEXT NOT NULL,
                bio         TEXT,
                location    TEXT
            );
            INSERT INTO user (
                id, password, bio, location
            ) VALUES (
                'root',
                'alpine',
                'root account',
                ''
            );
            COMMIT;
        ",
        )
        .unwrap();
    } else {
        panic!("failed to create a table");
    }

    // We're good to go
    while let Ok(s) = listener.accept().await {
        println!("New connection from: {:?}", s.0);
        tokio::spawn(session_task(
            s.0,
            msg_tx.clone(),
            state.clone(),
            sqlconn.clone(),
        ));
    }
    Ok(())
}
