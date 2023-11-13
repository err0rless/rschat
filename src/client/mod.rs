use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};

use crate::{db, packet::*};

pub mod background_task;
pub mod command;
pub mod input_controller;
pub mod input_handler;
pub mod session;
pub mod tui;
pub mod util;

pub async fn run_client(port: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Establish a connection and split into two unidirectional streams
    let (rd, wr) = match TcpStream::connect(format!("0.0.0.0:{}", port)).await {
        Ok(s) => tokio::io::split(s),
        Err(e) => panic!("'{}'", e),
    };

    // Channel for messages will be sent to the server
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<String>(32);

    // Channel for messages received
    let (incoming_tx, _) = broadcast::channel::<String>(32);

    // Task for comsuming the outgoing channel
    tokio::task::spawn(background_task::consume_outgoings(wr, outgoing_rx));

    // Task for reading TcpStream and enqueueing the messages to the channel
    tokio::task::spawn(background_task::produce_incomings(rd, incoming_tx.clone()));

    // Handshaking server for retrieveing temporary ID
    let id = {
        outgoing_tx
            .send(
                LoginReq {
                    // You are a guest when once join the server
                    login_info: db::user::Login::guest(),
                }
                .as_json_string(),
            )
            .await?;
        match util::consume_til::<LoginRes>(incoming_tx.subscribe())
            .await
            .result
        {
            Ok(r) => r,
            Err(s) => panic!("{}", s),
        }
    };

    let state = session::State::new_guest(id.as_str());

    let app = tui::App::new(outgoing_tx.clone(), incoming_tx.clone(), state);
    tui::set_tui(app).await?;
    Ok(())
}
