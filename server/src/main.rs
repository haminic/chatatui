use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use server::{handle_connection, ServerState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8054".to_string());

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
