mod client;
mod packet;
mod server;

const DEFAULT_PORT_NUM: &str = "8080";

#[tokio::main]
async fn main() {
    // running target: `client` or `server`
    let target = std::env::args()
        .nth(1)
        .unwrap_or(String::from("UNDEFINED_TARGET"));

    // port number
    let port = std::env::args()
        .nth(2)
        .unwrap_or(String::from(DEFAULT_PORT_NUM));

    // run the target
    match target.as_str() {
        "client" => _ = client::client::client_main(port).await,
        "server" => _ = server::server::server_main(port).await,
        _ => (),
    }
}
