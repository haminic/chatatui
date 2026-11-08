use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use server::{handle_connection, ServerState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", common::DEFAULT_PORT));
    let bind_addr = common::normalize_server_address(&raw_addr);

    let listener = TcpListener::bind(&bind_addr).await?;
    println!("Chatatui server listening on {}", listener.local_addr()?);

    let state = Arc::new(Mutex::new(ServerState::new()));

    loop {
        let (socket, _) = listener.accept().await?;
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            handle_connection(socket, state_clone).await;
        });
    }
}
