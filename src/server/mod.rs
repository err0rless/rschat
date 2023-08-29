use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;

use crate::packet::*;

pub mod session;

/// write `bytes` to the TCP stream with size header
async fn send_sized_bytes(
    wr: &mut WriteHalf<TcpStream>,
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    // super simple message protocol [Size: u32][Message: bytes]
    let mut writer = Vec::new();
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(bytes).await?;
    wr.write_all(&writer).await?;
    Ok(())
}

/// Consume messages from `sock_rx` channel and write them to `wr` directly
async fn stream_sender(mut wr: WriteHalf<TcpStream>, mut sock_rx: mpsc::Receiver<Vec<u8>>) {
    loop {
        if let Some(bytes) = sock_rx.recv().await {
            _ = send_sized_bytes(&mut wr, bytes.as_slice()).await;
        }
    }
}

/// Consumer for the channel `msg_rx`
///
/// This task can be gracefully terminated by notifying the `cancel_token`.
async fn message_handler(
    mut channel_tx: broadcast::Receiver<PacketType>,
    sock_tx: mpsc::Sender<Vec<u8>>,
    cancel_token: CancellationToken,
    id: Arc<Mutex<String>>,
) {
    let connected = AtomicBool::new(false);
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                break
            }
            message = channel_tx.recv() => match message {
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
            }
        }
    }
}

async fn response_handler(
    mut res_rx: mpsc::Receiver<PacketType>,
    sock_tx: mpsc::Sender<Vec<u8>>,
    id: Arc<Mutex<String>>,
) {
    loop {
        match res_rx.recv().await {
            Some(PacketType::RegisterRes(r)) => {
                _ = sock_tx.send(r.as_json_bytes()).await;
            }
            Some(PacketType::LoginRes(mut r)) => {
                // Login was successful, update the id
                if let Ok(mut lock) = id.lock() {
                    if let Ok(login_id) = &r.result {
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
            Some(PacketType::GotoRes(r)) => {
                _ = sock_tx.send(r.as_json_bytes()).await;
            }
            _ => (),
        }
    }
}

// Handler for each connection
async fn session_task(
    stream: TcpStream,
    channels: Arc<AsyncMutex<session::Channels>>,
    sqlconn: Arc<Mutex<rusqlite::Connection>>,
) {
    // Split into two unidirectional stream
    let (mut rd, wr) = tokio::io::split(stream);

    // Thread-safe id container
    let id = Arc::new(Mutex::new(String::new()));

    // Channel for consuming and send to the TCP stream
    let (sock_tx, sock_rx) = mpsc::channel::<Vec<u8>>(32);
    tokio::task::spawn(stream_sender(wr, sock_rx));

    // Channel for sending response back to client, or any type of packet that needs to be sent
    // to only current client
    let (res_tx, res_rx) = mpsc::channel::<PacketType>(32);
    tokio::task::spawn(response_handler(res_rx, sock_tx.clone(), Arc::clone(&id)));

    // default meessage channel
    let mut channel_tx = channels
        .lock()
        .await
        .get_channel(session::DEFAULT_CHANNEL)
        .expect("Failed to get default channel");

    // channel name container
    let mut current_channel: String = session::DEFAULT_CHANNEL.to_owned();

    // Default channel broadcasting task, notify `cancel_token` to terminate this task gracefully
    // so current client can connect to other chatting channel
    let mut cancel_token = CancellationToken::new();
    tokio::task::spawn(message_handler(
        channel_tx.subscribe(),
        sock_tx.clone(),
        cancel_token.clone(),
        Arc::clone(&id),
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
                    result: req.user.insert(Arc::clone(&sqlconn)),
                };
                _ = res_tx.send(PacketType::RegisterRes(res)).await;
            }
            // Received a request to login
            Some(PacketType::LoginReq(req)) => {
                let res = LoginRes {
                    result: {
                        let mut channels_lock = channels.lock().await;
                        let channel = channels_lock
                            .get_mut(&current_channel)
                            .expect("Channel not found");
                        if req.login_info.guest {
                            channel.connect_guest()
                        } else {
                            channel.connect_user(
                                &req,
                                id.lock().unwrap().as_str(),
                                Arc::clone(&sqlconn),
                            )
                        }
                    },
                };
                // Send packets in case login was successful
                if res.result.is_ok() {
                    _ = channel_tx.send(PacketType::Message(Message::connection(
                        &res.clone().result.unwrap(),
                    )));
                    _ = channel_tx.send(PacketType::Connected(Connected {}));
                }
                _ = res_tx.send(PacketType::LoginRes(res)).await;
            }
            Some(PacketType::FetchReq(fetch)) => {
                let fetch_res = match fetch.item.as_str() {
                    "list" => {
                        let mut channels_lock = channels.lock().await;
                        let channel = channels_lock
                            .get_mut(&current_channel)
                            .expect("Channel not found");
                        FetchRes {
                            item: fetch.item,
                            result: Ok(serde_json::json!({
                                "user_list": channel.user_list(),
                                "num_user": channel.num_user(),
                                "num_guest": channel.num_guest(),
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
            Some(PacketType::GotoReq(req)) => {
                let mut previous_channel_name = "".to_owned();
                let packet = PacketType::GotoRes(GotoRes {
                    result: match channels.lock().await.get_mut(req.channel_name.as_str()) {
                        Some(req_channel) => {
                            // save channel name and reassign
                            previous_channel_name = current_channel.clone();
                            current_channel = req.channel_name;

                            // notify the existing channel for termination and generate a new token
                            cancel_token.cancel();
                            cancel_token = CancellationToken::new();

                            // new broadcasting channel
                            channel_tx = req_channel.channel.clone();
                            tokio::task::spawn(message_handler(
                                channel_tx.subscribe(),
                                sock_tx.clone(),
                                cancel_token.clone(),
                                Arc::clone(&id),
                            ));
                            _ = channel_tx.send(PacketType::Connected(Connected {}));

                            // update state
                            if let Ok(lock) = id.lock() {
                                req_channel.add_connection(lock.as_str());
                                Ok(current_channel.clone())
                            } else {
                                Err("Failed to get identifier".to_owned())
                            }
                        }
                        None => Err("Invalid or not permitted to join the channel".to_owned()),
                    },
                });

                // FIXME: Mutex lock for `channels` is valid til the end of the above statement,
                // so we cannot update state of the current channel. Looks ugly.
                match &packet {
                    PacketType::GotoRes(res) if res.result.is_ok() => channels
                        .lock()
                        .await
                        .get_mut(previous_channel_name.as_str())
                        .expect("Channel not found")
                        .leave_user(id.lock().as_deref().unwrap()),
                    _ => (),
                };

                if let Err(e) = res_tx.send(packet).await {
                    println!("{}", e);
                }
            }
            // Received a request to broadcast message
            Some(PacketType::Message(msg)) => {
                // Send message to the channel for broadcasting to connected clients
                _ = channel_tx.send(PacketType::Message(msg));
            }
            // Received exit notification from client, remove the client from current session
            Some(PacketType::Exit(_)) => {
                let mut channels_lock = channels.lock().await;
                let channel = channels_lock
                    .get_mut(&current_channel)
                    .expect("Channel not found");

                if let Ok(lock) = id.lock() {
                    channel.leave_user(lock.as_str());

                    // disconnection broadcasting
                    _ = channel_tx.send(PacketType::Message(Message::disconnection(&lock.clone())));
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

    // Chatting channel list
    let channels = Arc::new(AsyncMutex::new(session::Channels::with_system_channels()));

    // In-memory sqlite instance
    let sqlconn = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory()?));

    // Create essential tables / columns
    sqlconn
        .lock()
        .map(|lock| {
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
            .expect("failed to create an essential table")
        })
        .expect("somehow failed to lock sqlconn");

    // We're good to go
    while let Ok(s) = listener.accept().await {
        println!("New connection from: {:?}", s.0);
        tokio::spawn(session_task(
            s.0,
            Arc::clone(&channels),
            Arc::clone(&sqlconn),
        ));
    }
    Ok(())
}
