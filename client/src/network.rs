use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use common::{ClientMessage, ServerMessage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkEvent {
    Connected,
    Server(ServerMessage),
    Error(String),
    Disconnected(String),
}

/// Spawns background asynchronous network worker tasks for handling socket I/O.
pub fn spawn_network_worker(
    addr: String,
    mut outbound_rx: UnboundedReceiver<ClientMessage>,
    inbound_tx: UnboundedSender<NetworkEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let stream = match TcpStream::connect(&addr).await {
            Ok(s) => {
                let _ = inbound_tx.send(NetworkEvent::Connected);
                s
            }
            Err(err) => {
                let _ = inbound_tx.send(NetworkEvent::Error(format!(
                    "Failed to connect to {addr}: {err}"
                )));
                return;
            }
        };

        let (reader, mut writer) = stream.into_split();

        // Spawn outbound writer task
        let writer_handle = tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                match serde_json::to_string(&msg) {
                    Ok(mut json) => {
                        json.push('\n');
                        if let Err(err) = writer.write_all(json.as_bytes()).await {
                            eprintln!("Socket write error: {err}");
                            break;
                        }
                        if let Err(err) = writer.flush().await {
                            eprintln!("Socket flush error: {err}");
                            break;
                        }
                    }
                    Err(err) => eprintln!("Serialization error: {err}"),
                }
            }
        });

        // Inbound reader loop
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<ServerMessage>(line) {
                        Ok(msg) => {
                            if inbound_tx.send(NetworkEvent::Server(msg)).is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            let _ = inbound_tx.send(NetworkEvent::Error(format!(
                                "Failed to parse server message: {err}"
                            )));
                        }
                    }
                }
                Ok(None) => {
                    // Socket closed / EOF
                    break;
                }
                Err(err) => {
                    let _ = inbound_tx.send(NetworkEvent::Error(format!("Read error: {err}")));
                    break;
                }
            }
        }

        writer_handle.abort();
        let _ = inbound_tx.send(NetworkEvent::Disconnected(
            "Connection closed by server".to_string(),
        ));
    })
}
