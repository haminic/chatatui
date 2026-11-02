use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use common::{ClientMessage, ServerMessage, validate_name};

pub type ClientId = u64;

/// Errors that can occur during server message processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerError {
    ClientNotFound,
    AlreadyRegistered,
    NotRegistered,
    InvalidName(String),
    UsernameTaken(String),
    UserNotFound(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::ClientNotFound => write!(f, "Client not found"),
            ServerError::AlreadyRegistered => write!(f, "Client is already registered"),
            ServerError::NotRegistered => write!(f, "Must register before sending messages"),
            ServerError::InvalidName(err) => write!(f, "{err}"),
            ServerError::UsernameTaken(name) => write!(f, "Username '{name}' is already taken"),
            ServerError::UserNotFound(name) => write!(f, "User '{name}' not found"),
        }
    }
}

impl std::error::Error for ServerError {}

/// Represents an active client connection.
pub struct ClientSession {
    pub id: ClientId,
    pub username: Option<String>,
    pub tx: UnboundedSender<ServerMessage>,
}

/// In-memory state of the chat server.
/// Decoupled from transport (TCP) for testability.
pub struct ServerState {
    clients: HashMap<ClientId, ClientSession>,
    users: HashMap<String, ClientId>,
    next_client_id: ClientId,
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            users: HashMap::new(),
            next_client_id: 1,
        }
    }

    /// Registers a newly connected client session and returns its unique ClientId.
    pub fn connect_client(&mut self, tx: UnboundedSender<ServerMessage>) -> ClientId {
        let id = self.next_client_id;
        self.next_client_id += 1;

        self.clients.insert(
            id,
            ClientSession {
                id,
                username: None,
                tx,
            },
        );
        id
    }

    /// Handles client disconnection, removing the user and notifying remaining clients.
    pub fn disconnect_client(&mut self, client_id: ClientId) -> Option<String> {
        let session = self.clients.remove(&client_id)?;
        if let Some(username) = session.username {
            self.users.remove(&username);
            // Broadcast updated user list to all remaining registered clients
            let user_list = self.get_user_list();
            self.broadcast_to_registered(ServerMessage::UserList { users: user_list });
            Some(username)
        } else {
            None
        }
    }

    /// Asserts that a client is registered with a username.
    /// Returns the registered username on success, or ServerError if not registered or not found.
    pub fn assert_registered(&self, client_id: ClientId) -> Result<String, ServerError> {
        let session = self
            .clients
            .get(&client_id)
            .ok_or(ServerError::ClientNotFound)?;
        session.username.clone().ok_or(ServerError::NotRegistered)
    }

    /// Handles an incoming message from a client.
    /// Dispatches to the appropriate handler. If the handler returns an Err,
    /// sends a ServerMessage::Error back to the client.
    pub fn handle_message(
        &mut self,
        client_id: ClientId,
        msg: ClientMessage,
    ) -> Result<(), ServerError> {
        let result = match msg {
            ClientMessage::Register { username } => self.handle_register(client_id, username),
            ClientMessage::DirectMessage { recipient, content } => {
                self.handle_direct_message(client_id, recipient, content)
            }
        };

        if let Err(ref err) = result {
            self.send_to(
                client_id,
                ServerMessage::Error {
                    message: err.to_string(),
                },
            );
        }

        result
    }

    /// Handles client registration with username validation and duplicate checking.
    pub fn handle_register(
        &mut self,
        client_id: ClientId,
        username: String,
    ) -> Result<(), ServerError> {
        let session = self
            .clients
            .get(&client_id)
            .ok_or(ServerError::ClientNotFound)?;

        if session.username.is_some() {
            return Err(ServerError::AlreadyRegistered);
        }

        validate_name(&username).map_err(|e| ServerError::InvalidName(e.to_string()))?;

        if self.users.contains_key(&username) {
            return Err(ServerError::UsernameTaken(username));
        }

        if let Some(session) = self.clients.get_mut(&client_id) {
            session.username = Some(username.clone());
        }
        self.users.insert(username.clone(), client_id);

        self.send_to(
            client_id,
            ServerMessage::RegisterSuccess {
                username: username.clone(),
            },
        );

        let users = self.get_user_list();
        self.broadcast_to_registered(ServerMessage::UserList { users });

        Ok(())
    }

    /// Handles routing direct messages between registered users.
    pub fn handle_direct_message(
        &mut self,
        client_id: ClientId,
        recipient: String,
        content: String,
    ) -> Result<(), ServerError> {
        let sender_username = self.assert_registered(client_id)?;

        let recipient_id = self
            .users
            .get(&recipient)
            .copied()
            .ok_or_else(|| ServerError::UserNotFound(recipient.clone()))?;

        self.send_to(
            recipient_id,
            ServerMessage::DirectMessage {
                from: sender_username,
                to: recipient,
                content,
            },
        );

        Ok(())
    }

    /// Sends a message to a specific client if connected.
    pub fn send_to(&self, client_id: ClientId, msg: ServerMessage) {
        if let Some(session) = self.clients.get(&client_id) {
            let _ = session.tx.send(msg);
        }
    }

    /// Broadcasts a message to all currently registered clients.
    pub fn broadcast_to_registered(&self, msg: ServerMessage) {
        for session in self.clients.values() {
            if session.username.is_some() {
                let _ = session.tx.send(msg.clone());
            }
        }
    }

    /// Returns a sorted list of all active registered usernames.
    pub fn get_user_list(&self) -> Vec<String> {
        let mut list: Vec<String> = self.users.keys().cloned().collect();
        list.sort();
        list
    }

    /// Returns the total number of connected clients (registered or unregistered).
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Returns the total number of registered users.
    pub fn registered_user_count(&self) -> usize {
        self.users.len()
    }

    /// Checks if a username is currently registered.
    pub fn is_registered(&self, username: &str) -> bool {
        self.users.contains_key(username)
    }
}

/// Handles an incoming TCP connection: sets up reader and writer channels,
/// handles client framing with line-delimited JSON, and cleans up on disconnect.
pub async fn handle_connection(socket: TcpStream, state: Arc<Mutex<ServerState>>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let client_id = state.lock().await.connect_client(tx);

    let (reader, mut writer) = socket.into_split();

    // Outbound writer task
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(mut json) = serde_json::to_string(&msg) {
                json.push('\n');
                if let Err(err) = writer.write_all(json.as_bytes()).await {
                    eprintln!("[Client {client_id}] socket write failed: {err}");
                    break;
                }
            }
        }
    });

    // Inbound reader loop
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<ClientMessage>(line) {
            Ok(msg) => {
                let mut s = state.lock().await;
                let _ = s.handle_message(client_id, msg);
            }
            Err(err) => {
                let s = state.lock().await;
                s.send_to(
                    client_id,
                    ServerMessage::Error {
                        message: format!("Malformed message: {err}"),
                    },
                );
            }
        }
    }

    // Clean up on disconnect
    state.lock().await.disconnect_client(client_id);
    writer_task.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn setup_client(
        server: &mut ServerState,
    ) -> (ClientId, mpsc::UnboundedReceiver<ServerMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let client_id = server.connect_client(tx);
        (client_id, rx)
    }

    #[test]
    fn test_client_connect_and_disconnect_unregistered() {
        let mut server = ServerState::new();
        let (client_id, _) = setup_client(&mut server);
        assert_eq!(server.client_count(), 1);
        assert_eq!(server.registered_user_count(), 0);

        let disconnected_user = server.disconnect_client(client_id);
        assert_eq!(disconnected_user, None);
        assert_eq!(server.client_count(), 0);
    }

    #[test]
    fn test_assert_registered() {
        let mut server = ServerState::new();
        let (client_id, _) = setup_client(&mut server);

        // Not registered yet
        assert_eq!(
            server.assert_registered(client_id),
            Err(ServerError::NotRegistered)
        );

        // Nonexistent client
        assert_eq!(
            server.assert_registered(9999),
            Err(ServerError::ClientNotFound)
        );

        // After registration
        let res = server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert!(res.is_ok());
        assert_eq!(server.assert_registered(client_id), Ok("alice".to_string()));
    }

    #[test]
    fn test_register_success() {
        let mut server = ServerState::new();
        let (client_id, mut rx) = setup_client(&mut server);

        let res = server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert_eq!(res, Ok(()));

        assert_eq!(
            rx.try_recv().unwrap(),
            ServerMessage::RegisterSuccess {
                username: "alice".to_string()
            }
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            ServerMessage::UserList {
                users: vec!["alice".to_string()]
            }
        );
        assert!(rx.try_recv().is_err());
        assert!(server.is_registered("alice"));
    }

    #[test]
    fn test_register_invalid_name() {
        let mut server = ServerState::new();
        let (client_id, mut rx) = setup_client(&mut server);

        // Too short
        let res = server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "al".to_string(),
            },
        );
        assert!(matches!(res, Err(ServerError::InvalidName(_))));
        match rx.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("at least 3 characters"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }

        // Uppercase not allowed
        let res = server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "Alice".to_string(),
            },
        );
        assert!(matches!(res, Err(ServerError::InvalidName(_))));
        match rx.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("[a-z0-9_]"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }

        assert_eq!(server.registered_user_count(), 0);
    }

    #[test]
    fn test_register_duplicate() {
        let mut server = ServerState::new();
        let (c1, mut rx1) = setup_client(&mut server);
        let (c2, mut rx2) = setup_client(&mut server);

        // Client 1 registers as "alice"
        let res1 = server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert_eq!(res1, Ok(()));
        assert!(matches!(
            rx1.try_recv().unwrap(),
            ServerMessage::RegisterSuccess { .. }
        ));
        assert!(matches!(
            rx1.try_recv().unwrap(),
            ServerMessage::UserList { .. }
        ));

        // Client 2 attempts to also register as "alice"
        let res2 = server.handle_message(
            c2,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert_eq!(res2, Err(ServerError::UsernameTaken("alice".to_string())));
        match rx2.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("already taken"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }
    }

    #[test]
    fn test_register_twice_same_session() {
        let mut server = ServerState::new();
        let (c1, mut rx1) = setup_client(&mut server);

        let res1 = server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert_eq!(res1, Ok(()));
        let _ = rx1.try_recv(); // RegisterSuccess
        let _ = rx1.try_recv(); // UserList

        // Attempt second registration on same session
        let res2 = server.handle_message(
            c1,
            ClientMessage::Register {
                username: "bob".to_string(),
            },
        );
        assert_eq!(res2, Err(ServerError::AlreadyRegistered));
        match rx1.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("already registered"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }
    }

    #[test]
    fn test_direct_message_success() {
        let mut server = ServerState::new();
        let (c1, mut rx1) = setup_client(&mut server);
        let (c2, mut rx2) = setup_client(&mut server);

        assert!(
            server
                .handle_message(
                    c1,
                    ClientMessage::Register {
                        username: "alice".to_string(),
                    },
                )
                .is_ok()
        );
        assert!(
            server
                .handle_message(
                    c2,
                    ClientMessage::Register {
                        username: "bob".to_string(),
                    },
                )
                .is_ok()
        );

        // Drain registration messages
        while rx1.try_recv().is_ok() {}
        while rx2.try_recv().is_ok() {}

        // Alice sends direct message to Bob
        let res = server.handle_message(
            c1,
            ClientMessage::DirectMessage {
                recipient: "bob".to_string(),
                content: "hello bob!".to_string(),
            },
        );
        assert_eq!(res, Ok(()));

        // Bob should receive the DM
        assert_eq!(
            rx2.try_recv().unwrap(),
            ServerMessage::DirectMessage {
                from: "alice".to_string(),
                to: "bob".to_string(),
                content: "hello bob!".to_string(),
            }
        );

        // Alice should receive nothing (no error)
        assert!(rx1.try_recv().is_err());
    }

    #[test]
    fn test_direct_message_user_not_found() {
        let mut server = ServerState::new();
        let (c1, mut rx1) = setup_client(&mut server);

        assert!(
            server
                .handle_message(
                    c1,
                    ClientMessage::Register {
                        username: "alice".to_string(),
                    },
                )
                .is_ok()
        );
        while rx1.try_recv().is_ok() {}

        // Send DM to nonexistent user
        let res = server.handle_message(
            c1,
            ClientMessage::DirectMessage {
                recipient: "charlie".to_string(),
                content: "hey".to_string(),
            },
        );
        assert_eq!(res, Err(ServerError::UserNotFound("charlie".to_string())));

        match rx1.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("User 'charlie' not found"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }
    }

    #[test]
    fn test_direct_message_unregistered_sender() {
        let mut server = ServerState::new();
        let (c1, mut rx1) = setup_client(&mut server);

        let res = server.handle_message(
            c1,
            ClientMessage::DirectMessage {
                recipient: "bob".to_string(),
                content: "hey".to_string(),
            },
        );
        assert_eq!(res, Err(ServerError::NotRegistered));

        match rx1.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("Must register"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }
    }

    #[test]
    fn test_disconnect_registered_updates_user_list() {
        let mut server = ServerState::new();
        let (c1, mut rx1) = setup_client(&mut server);
        let (c2, mut rx2) = setup_client(&mut server);

        assert!(
            server
                .handle_message(
                    c1,
                    ClientMessage::Register {
                        username: "alice".to_string(),
                    },
                )
                .is_ok()
        );
        assert!(
            server
                .handle_message(
                    c2,
                    ClientMessage::Register {
                        username: "bob".to_string(),
                    },
                )
                .is_ok()
        );

        while rx1.try_recv().is_ok() {}
        while rx2.try_recv().is_ok() {}

        // Alice disconnects
        let removed = server.disconnect_client(c1);
        assert_eq!(removed, Some("alice".to_string()));

        // Bob should receive updated user list containing only "bob"
        assert_eq!(
            rx2.try_recv().unwrap(),
            ServerMessage::UserList {
                users: vec!["bob".to_string()]
            }
        );

        // "alice" username can now be registered again
        let (c3, mut rx3) = setup_client(&mut server);
        let res = server.handle_message(
            c3,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert_eq!(res, Ok(()));
        assert_eq!(
            rx3.try_recv().unwrap(),
            ServerMessage::RegisterSuccess {
                username: "alice".to_string()
            }
        );
    }
}
