mod client;
mod crypto;
mod db;
mod packet;
mod server;

const DEFAULT_PORT_NUM: &str = "8080";

fn usage() {
    println!("Usage: ./rschat 'target'");
    println!("   available targets: 'client', 'server'");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // running target: `client` or `server`
    let target = std::env::args().nth(1);

    // port number
    let port = std::env::args()
        .nth(2)
        .unwrap_or(DEFAULT_PORT_NUM.to_owned());

    // run the target
    match target.as_deref() {
        Some("client") => client::run_client(port.as_str()).await?,
        Some("server") => server::run_server(port.as_str()).await?,
        _ => usage(),
    }
    Ok(())
}
