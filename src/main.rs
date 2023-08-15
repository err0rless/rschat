mod client;
mod crypto;
mod db;
mod packet;
mod server;

const DEFAULT_PORT_NUM: &str = "8080";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // running target: `client` or `server`
    let target = std::env::args().nth(1).unwrap();

    // port number
    let port = std::env::args()
        .nth(2)
        .unwrap_or(String::from(DEFAULT_PORT_NUM));

    // run the target
    match target.as_str() {
        "client" => client::client::run_client(port).await?,
        "server" => server::server::run_server(port).await?,
        t => panic!("Unknown target: '{t}'"),
    }
    Ok(())
}
