use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};

use client::app::App;
use client::network::{spawn_network_worker, NetworkEvent};
use common::{ClientMessage, ServerMessage};
use server::{handle_connection, ServerState};

#[tokio::test]
async fn test_client_network_worker_dm_and_disconnect_wiping() {
    // 1. Bind test server on ephemeral port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap().to_string();
    let state = Arc::new(Mutex::new(ServerState::new()));

    let s = Arc::clone(&state);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let s_clone = Arc::clone(&s);
            tokio::spawn(async move {
                handle_connection(socket, s_clone).await;
            });
        }
    });

    // 2. Connect Alice
    let (alice_out_tx, alice_out_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (alice_in_tx, mut alice_in_rx) = mpsc::unbounded_channel::<NetworkEvent>();
    let mut alice_app = App::new();
    alice_app.username_input = "alice".to_string();

    let alice_handle = spawn_network_worker(server_addr.clone(), alice_out_rx, alice_in_tx);

    // Wait for Alice connected
    let ev = tokio::time::timeout(Duration::from_secs(2), alice_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ev, NetworkEvent::Connected);

    // Alice registers
    alice_out_tx
        .send(ClientMessage::Register {
            username: "alice".to_string(),
        })
        .unwrap();

    // Alice receives RegisterSuccess
    let ev = tokio::time::timeout(Duration::from_secs(2), alice_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::RegisterSuccess { .. }) => {
            alice_app.handle_server_message(msg);
        }
        other => panic!("Expected RegisterSuccess, got {:?}", other),
    }

    // Alice receives UserList
    let ev = tokio::time::timeout(Duration::from_secs(2), alice_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::UserList { .. }) => {
            alice_app.handle_server_message(msg);
        }
        other => panic!("Expected UserList, got {:?}", other),
    }
    assert_eq!(alice_app.users, Vec::<String>::new());

    // 3. Connect Bob
    let (bob_out_tx, bob_out_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (bob_in_tx, mut bob_in_rx) = mpsc::unbounded_channel::<NetworkEvent>();
    let mut bob_app = App::new();
    bob_app.username_input = "bob".to_string();

    let _bob_handle = spawn_network_worker(server_addr.clone(), bob_out_rx, bob_in_tx);

    let ev = tokio::time::timeout(Duration::from_secs(2), bob_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ev, NetworkEvent::Connected);

    // Bob registers
    bob_out_tx
        .send(ClientMessage::Register {
            username: "bob".to_string(),
        })
        .unwrap();

    // Bob receives RegisterSuccess
    let ev = tokio::time::timeout(Duration::from_secs(2), bob_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::RegisterSuccess { .. }) => {
            bob_app.handle_server_message(msg);
        }
        other => panic!("Expected RegisterSuccess, got {:?}", other),
    }

    // Bob receives UserList
    let ev = tokio::time::timeout(Duration::from_secs(2), bob_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::UserList { .. }) => {
            bob_app.handle_server_message(msg);
        }
        other => panic!("Expected UserList, got {:?}", other),
    }
    assert_eq!(bob_app.users, vec!["alice".to_string()]);

    // Alice receives updated UserList with Bob
    let ev = tokio::time::timeout(Duration::from_secs(2), alice_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::UserList { .. }) => {
            alice_app.handle_server_message(msg);
        }
        other => panic!("Expected UserList, got {:?}", other),
    }
    assert_eq!(alice_app.users, vec!["bob".to_string()]);
    assert_eq!(alice_app.highlighted_user(), Some("bob"));

    // 4. Alice opens chat with Bob and sends DM
    alice_app.open_selected_chat();
    assert_eq!(alice_app.active_chat(), Some("bob"));

    alice_app.input_message = "hello bob!".to_string();
    let dm_msg = alice_app.send_current_message().unwrap();
    alice_out_tx.send(dm_msg).unwrap();

    // Verify Alice has local chat history for Bob with pending status
    let alice_history = alice_app.chat_histories.get("bob").unwrap();
    assert_eq!(alice_history.len(), 1);
    assert_eq!(alice_history[0].content, "hello bob!");
    assert!(alice_history[0].is_me);
    assert!(alice_history[0].is_pending);
    assert_eq!(alice_history[0].timestamp, None);

    // Alice receives sender echo
    let ev = tokio::time::timeout(Duration::from_secs(2), alice_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::DirectMessage { .. }) => {
            alice_app.handle_server_message(msg);
        }
        other => panic!("Expected sender echo DirectMessage, got {:?}", other),
    }

    // Verify Alice's pending message is resolved
    let alice_history = alice_app.chat_histories.get("bob").unwrap();
    assert_eq!(alice_history.len(), 1);
    assert!(!alice_history[0].is_pending);
    assert!(alice_history[0].timestamp.is_some());

    // Bob receives DM
    let ev = tokio::time::timeout(Duration::from_secs(2), bob_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::DirectMessage { .. }) => {
            bob_app.handle_server_message(msg);
        }
        other => panic!("Expected DirectMessage, got {:?}", other),
    }

    // Verify Bob has chat history for Alice
    let bob_history = bob_app.chat_histories.get("alice").unwrap();
    assert_eq!(bob_history.len(), 1);
    assert_eq!(bob_history[0].content, "hello bob!");
    assert!(!bob_history[0].is_me);
    assert!(!bob_history[0].is_pending);
    assert!(bob_history[0].timestamp.is_some());

    // 5. Alice disconnects
    drop(alice_out_tx);
    alice_handle.abort();

    // Bob receives updated UserList without Alice
    let ev = tokio::time::timeout(Duration::from_secs(2), bob_in_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        NetworkEvent::Server(msg @ ServerMessage::UserList { .. }) => {
            bob_app.handle_server_message(msg);
        }
        other => panic!("Expected UserList, got {:?}", other),
    }

    // Bob's app must have wiped Alice's chat history per SPECS.md!
    assert_eq!(bob_app.users, Vec::<String>::new());
    assert!(!bob_app.chat_histories.contains_key("alice"));
}

#[tokio::test]
async fn test_client_registration_error_taken() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap().to_string();
    let state = Arc::new(Mutex::new(ServerState::new()));

    let s = Arc::clone(&state);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let s_clone = Arc::clone(&s);
            tokio::spawn(async move {
                handle_connection(socket, s_clone).await;
            });
        }
    });

    // 1. First client registers as "charlie"
    let (tx1, rx1) = mpsc::unbounded_channel();
    let (in_tx1, mut in_rx1) = mpsc::unbounded_channel();
    let _h1 = spawn_network_worker(server_addr.clone(), rx1, in_tx1);
    let _ = in_rx1.recv().await; // Connected
    tx1.send(ClientMessage::Register {
        username: "charlie".to_string(),
    })
    .unwrap();
    let _ = in_rx1.recv().await; // RegisterSuccess
    let _ = in_rx1.recv().await; // UserList

    // 2. Second client attempts to also register as "charlie"
    let (tx2, rx2) = mpsc::unbounded_channel();
    let (in_tx2, mut in_rx2) = mpsc::unbounded_channel();
    let mut app2 = App::new();
    app2.username_input = "charlie".to_string();
    app2.is_connecting = true;

    let _h2 = spawn_network_worker(server_addr.clone(), rx2, in_tx2);
    let _ = in_rx2.recv().await; // Connected
    tx2.send(ClientMessage::Register {
        username: "charlie".to_string(),
    })
    .unwrap();

    let ev = tokio::time::timeout(Duration::from_secs(2), in_rx2.recv())
        .await
        .unwrap()
        .unwrap();

    match ev {
        NetworkEvent::Server(msg @ ServerMessage::Error { .. }) => {
            app2.handle_server_message(msg);
        }
        other => panic!("Expected ServerMessage::Error, got {:?}", other),
    }

    assert_eq!(app2.screen, client::app::Screen::Connect);
    assert!(!app2.is_connecting);
    assert!(app2.connect_error.as_ref().unwrap().contains("already taken"));
}
