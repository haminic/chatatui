use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use common::{ClientMessage, ServerMessage};
use server::{ServerState, handle_connection};

async fn send_msg<W: AsyncWriteExt + Unpin>(writer: &mut W, msg: &ClientMessage) {
    let mut json = serde_json::to_string(msg).unwrap();
    json.push('\n');
    writer.write_all(json.as_bytes()).await.unwrap();
}

async fn read_msg(
    reader: &mut tokio::io::Lines<BufReader<tokio::net::tcp::ReadHalf<'_>>>,
) -> ServerMessage {
    let line = reader
        .next_line()
        .await
        .unwrap()
        .expect("expected line, got EOF");
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn test_tcp_server_registration_dm_and_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let state = Arc::new(Mutex::new(ServerState::new()));

    // Spawn server loop
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let s = Arc::clone(&state_clone);
            tokio::spawn(async move {
                handle_connection(socket, s).await;
            });
        }
    });

    // 1. Alice connects and registers
    let mut alice_stream = TcpStream::connect(local_addr).await.unwrap();
    let (alice_read, mut alice_write) = alice_stream.split();
    let mut alice_lines = BufReader::new(alice_read).lines();

    send_msg(
        &mut alice_write,
        &ClientMessage::Register {
            username: "alice".to_string(),
        },
    )
    .await;

    let msg = read_msg(&mut alice_lines).await;
    assert_eq!(
        msg,
        ServerMessage::RegisterSuccess {
            username: "alice".to_string()
        }
    );

    let msg = read_msg(&mut alice_lines).await;
    assert_eq!(
        msg,
        ServerMessage::UserList {
            users: vec!["alice".to_string()]
        }
    );

    // 2. Bob connects and registers
    let mut bob_stream = TcpStream::connect(local_addr).await.unwrap();
    let (bob_read, mut bob_write) = bob_stream.split();
    let mut bob_lines = BufReader::new(bob_read).lines();

    send_msg(
        &mut bob_write,
        &ClientMessage::Register {
            username: "bob".to_string(),
        },
    )
    .await;

    let msg = read_msg(&mut bob_lines).await;
    assert_eq!(
        msg,
        ServerMessage::RegisterSuccess {
            username: "bob".to_string()
        }
    );

    let msg = read_msg(&mut bob_lines).await;
    assert_eq!(
        msg,
        ServerMessage::UserList {
            users: vec!["alice".to_string(), "bob".to_string()]
        }
    );

    // Alice should also receive the updated user list now that Bob joined
    let msg = read_msg(&mut alice_lines).await;
    assert_eq!(
        msg,
        ServerMessage::UserList {
            users: vec!["alice".to_string(), "bob".to_string()]
        }
    );

    // 3. Alice sends a DM to Bob
    send_msg(
        &mut alice_write,
        &ClientMessage::DirectMessage {
            recipient: "bob".to_string(),
            content: "hello over tcp!".to_string(),
        },
    )
    .await;

    // Bob receives DM
    let bob_dm = read_msg(&mut bob_lines).await;
    let ts = match bob_dm {
        ServerMessage::DirectMessage {
            ref from,
            ref to,
            ref content,
            timestamp,
        } => {
            assert_eq!(from, "alice");
            assert_eq!(to, "bob");
            assert_eq!(content, "hello over tcp!");
            assert!(timestamp > 0);
            timestamp
        }
        other => panic!("Expected DirectMessage, got {:?}", other),
    };

    // Alice receives sender echo
    let alice_echo = read_msg(&mut alice_lines).await;
    assert_eq!(
        alice_echo,
        ServerMessage::DirectMessage {
            from: "alice".to_string(),
            to: "bob".to_string(),
            content: "hello over tcp!".to_string(),
            timestamp: ts,
        }
    );

    // 4. Alice disconnects
    drop(alice_write);
    drop(alice_lines);
    drop(alice_stream);

    // Bob receives updated user list with Alice removed
    let msg = read_msg(&mut bob_lines).await;
    assert_eq!(
        msg,
        ServerMessage::UserList {
            users: vec!["bob".to_string()]
        }
    );
}
