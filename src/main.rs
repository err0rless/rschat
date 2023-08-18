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
    let target = if let Some(t) = std::env::args().nth(1) {
        t
    } else {
        usage();
        return Ok(());
    };

    // port number
    let port = std::env::args()
        .nth(2)
        .unwrap_or(String::from(DEFAULT_PORT_NUM));

    // run the target
    match target.as_str() {
        "client" => client::client::run_client(port).await?,
        "server" => server::server::run_server(port).await?,
        _ => usage(),
    }
    Ok(())
}
