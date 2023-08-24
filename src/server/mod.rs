use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rand::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

use crate::packet::*;

pub mod session;

/// write `bytes` to the TCP stream with size header
async fn send_sized_bytes(wr: &mut WriteHalf<TcpStream>, bytes: &[u8]) {
    // super simple message protocol [Size: u32][Message: bytes]
    _ = wr.write_u32(bytes.len() as u32).await;
    _ = wr.write_all(bytes).await;
}

/// Consume the messages from `sock_rx` channel and write them to `wr` directly
async fn stream_sender(mut wr: WriteHalf<TcpStream>, mut sock_rx: mpsc::Receiver<Vec<u8>>) {
    loop {
        if let Some(bytes) = sock_rx.recv().await {
            _ = send_sized_bytes(&mut wr, bytes.as_slice()).await;
        }
    }
}

async fn channel_consumer(
    mut msg_rx: broadcast::Receiver<PacketType>,
    mut res_rx: mpsc::Receiver<PacketType>,
    sock_tx: mpsc::Sender<Vec<u8>>,
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
                    match id.lock() {
                        Ok(lock) if lock.as_str() == msg.id => continue,
                        Err(_) => continue,
                        Ok(_) => (),
                    }

                    // Write message to the stream
                    _ = sock_tx.send(msg.as_json_bytes()).await;
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
                    _ = sock_tx.send(r.as_json_bytes()).await;
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
                        r.result = Err("failed to login".to_owned());
                    }
                    _ = sock_tx.send(r.as_json_bytes()).await;
                }
                Some(PacketType::FetchRes(r)) => {
                    _ = sock_tx.send(r.as_json_bytes()).await;
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

    // Channel for consuming and send to the TCP stream
    let (sock_tx, sock_rx) = mpsc::channel::<Vec<u8>>(32);
    tokio::task::spawn(stream_sender(wr, sock_rx));

    // Channel consumer: consumes the messages from the channels and handle them
    //  - msg_rx: broadcast channel
    //  - res_rx: mpsc channel
    tokio::task::spawn(channel_consumer(
        msg_rx,
        res_rx,
        sock_tx.clone(),
        id.clone(),
    ));

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
                            break 'outer Err("unknown error!".to_owned());
                        };

                        if req.login_info.guest {
                            // Guets Login
                            if lock.num_guest >= session::NUM_MAX_GUEST {
                                break 'outer Err("too many guests".to_owned());
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
                                break 'outer Err("too many users".to_owned());
                            }

                            // validation of inputs was done before this packet reached here, but somehow it's broken
                            if req.login_info.id.is_none() || req.login_info.password.is_none() {
                                break 'outer Err("broken login packet".to_owned());
                            }

                            let res = req.login_info.login(sqlconn.clone());
                            if res.is_ok() {
                                lock.num_guest -= 1; // every connection is a guest at first
                                lock.num_user += 1;
                                lock.names.remove(id.lock().unwrap().as_str());
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
            Some(PacketType::FetchReq(fetch)) => {
                let fetch_res = match fetch.item.as_str() {
                    "list" => {
                        let info = if let Ok(lock) = state.lock() {
                            (
                                lock.names.iter().map(String::from).collect::<Vec<String>>(),
                                lock.num_user,
                                lock.num_guest,
                            )
                        } else {
                            continue;
                        };

                        FetchRes {
                            item: fetch.item,
                            result: Ok(serde_json::json!({
                                "user_list": info.0,
                                "num_user": info.1,
                                "num_guest": info.2,
                            })),
                        }
                    }
                    // Handling unknown fetch items
                    _ => FetchRes {
                        item: fetch.item,
                        result: Err("unknown fetch item".to_owned()),
                    },
                };
                _ = res_tx.send(PacketType::FetchRes(fetch_res)).await;
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
