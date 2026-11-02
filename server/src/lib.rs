use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

use common::{ClientMessage, ServerMessage, validate_name};

pub type ClientId = u64;

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

    /// Handles an incoming message from a client.
    pub fn handle_message(&mut self, client_id: ClientId, msg: ClientMessage) {
        match msg {
            ClientMessage::Register { username } => {
                self.handle_register(client_id, username);
            }
            ClientMessage::DirectMessage { recipient, content } => {
                self.handle_direct_message(client_id, recipient, content);
            }
        }
    }

    fn handle_register(&mut self, client_id: ClientId, username: String) {
        // Validate client session exists
        let session = match self.clients.get(&client_id) {
            Some(s) => s,
            None => return,
        };

        // Reject if client already registered
        if session.username.is_some() {
            self.send_to(
                client_id,
                ServerMessage::Error {
                    message: "Client is already registered".to_string(),
                },
            );
            return;
        }

        // Validate username syntax
        if let Err(err_msg) = validate_name(&username) {
            self.send_to(
                client_id,
                ServerMessage::Error {
                    message: err_msg.to_string(),
                },
            );
            return;
        }

        // Check for duplicate username
        if self.users.contains_key(&username) {
            self.send_to(
                client_id,
                ServerMessage::Error {
                    message: format!("Username '{username}' is already taken"),
                },
            );
            return;
        }

        // Update session and user registry
        if let Some(session) = self.clients.get_mut(&client_id) {
            session.username = Some(username.clone());
        }
        self.users.insert(username.clone(), client_id);

        // Send confirmation to registered client
        self.send_to(
            client_id,
            ServerMessage::RegisterSuccess {
                username: username.clone(),
            },
        );

        // Broadcast updated user list to all registered clients
        let users = self.get_user_list();
        self.broadcast_to_registered(ServerMessage::UserList { users });
    }

    fn handle_direct_message(&mut self, client_id: ClientId, recipient: String, content: String) {
        // Find sender's username
        let sender_username = match self
            .clients
            .get(&client_id)
            .and_then(|s| s.username.clone())
        {
            Some(name) => name,
            None => {
                self.send_to(
                    client_id,
                    ServerMessage::Error {
                        message: "Must register before sending messages".to_string(),
                    },
                );
                return;
            }
        };

        // Find recipient's client id
        let recipient_id = match self.users.get(&recipient).copied() {
            Some(id) => id,
            None => {
                self.send_to(
                    client_id,
                    ServerMessage::Error {
                        message: format!("User '{recipient}' not found"),
                    },
                );
                return;
            }
        };

        // Forward message strictly to receiver
        self.send_to(
            recipient_id,
            ServerMessage::DirectMessage {
                from: sender_username,
                to: recipient,
                content,
            },
        );
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
    fn test_register_success() {
        let mut server = ServerState::new();
        let (client_id, mut rx) = setup_client(&mut server);

        server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );

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
        server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "al".to_string(),
            },
        );
        match rx.try_recv().unwrap() {
            ServerMessage::Error { message } => {
                assert!(message.contains("at least 3 characters"));
            }
            other => panic!("Expected Error message, got {:?}", other),
        }

        // Uppercase not allowed
        server.handle_message(
            client_id,
            ClientMessage::Register {
                username: "Alice".to_string(),
            },
        );
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
        server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert!(matches!(
            rx1.try_recv().unwrap(),
            ServerMessage::RegisterSuccess { .. }
        ));
        assert!(matches!(
            rx1.try_recv().unwrap(),
            ServerMessage::UserList { .. }
        ));

        // Client 2 attempts to also register as "alice"
        server.handle_message(
            c2,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
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

        server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        let _ = rx1.try_recv(); // RegisterSuccess
        let _ = rx1.try_recv(); // UserList

        // Attempt second registration on same session
        server.handle_message(
            c1,
            ClientMessage::Register {
                username: "bob".to_string(),
            },
        );
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

        server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        server.handle_message(
            c2,
            ClientMessage::Register {
                username: "bob".to_string(),
            },
        );

        // Drain registration messages
        while rx1.try_recv().is_ok() {}
        while rx2.try_recv().is_ok() {}

        // Alice sends direct message to Bob
        server.handle_message(
            c1,
            ClientMessage::DirectMessage {
                recipient: "bob".to_string(),
                content: "hello bob!".to_string(),
            },
        );

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

        server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        while rx1.try_recv().is_ok() {}

        // Send DM to nonexistent user
        server.handle_message(
            c1,
            ClientMessage::DirectMessage {
                recipient: "charlie".to_string(),
                content: "hey".to_string(),
            },
        );

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

        server.handle_message(
            c1,
            ClientMessage::DirectMessage {
                recipient: "bob".to_string(),
                content: "hey".to_string(),
            },
        );

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

        server.handle_message(
            c1,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        server.handle_message(
            c2,
            ClientMessage::Register {
                username: "bob".to_string(),
            },
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
        server.handle_message(
            c3,
            ClientMessage::Register {
                username: "alice".to_string(),
            },
        );
        assert_eq!(
            rx3.try_recv().unwrap(),
            ServerMessage::RegisterSuccess {
                username: "alice".to_string()
            }
        );
    }
}
