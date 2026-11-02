use std::collections::HashMap;

use common::{validate_name, ClientMessage, ServerMessage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
    pub is_me: bool,
    pub is_pending: bool,
    pub timestamp: Option<u64>,
}

impl ChatMessage {
    pub fn formatted_time(&self) -> Option<String> {
        self.timestamp.map(format_timestamp_12h)
    }
}

/// Formats a Unix epoch timestamp (seconds) into 12-hour format "HH:MM AM/PM" using local time.
pub fn format_timestamp_12h(ts: u64) -> String {
    let dt = chrono::DateTime::from_timestamp(ts as i64, 0).unwrap_or_default();
    let local = dt.with_timezone(&chrono::Local);
    local.format("%I:%M %p").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectField {
    ServerAddress,
    Username,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Connect,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Users,
    Input,
}

pub struct App {
    pub screen: Screen,
    pub connect_field: ConnectField,
    pub server_address: String,
    pub username_input: String,
    pub connect_error: Option<String>,
    pub is_connecting: bool,

    pub my_username: String,
    pub users: Vec<String>,
    /// The user index currently highlighted by navigation in the Left Panel.
    pub cursor_user_idx: Option<usize>,
    /// The user whose conversation is currently open in the Right Panel.
    pub active_chat_user: Option<String>,
    pub active_panel: ActivePanel,
    pub chat_histories: HashMap<String, Vec<ChatMessage>>,
    pub input_message: String,
    pub status_message: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Connect,
            connect_field: ConnectField::Username,
            server_address: "127.0.0.1:8054".to_string(),
            username_input: String::new(),
            connect_error: None,
            is_connecting: false,

            my_username: String::new(),
            users: Vec::new(),
            cursor_user_idx: None,
            active_chat_user: None,
            active_panel: ActivePanel::Users,
            chat_histories: HashMap::new(),
            input_message: String::new(),
            status_message: None,
        }
    }

    /// Validates connect inputs before attempting a socket connection.
    pub fn validate_connect_input(&self) -> Result<(), String> {
        if self.server_address.trim().is_empty() {
            return Err("Server address cannot be empty".to_string());
        }
        validate_name(&self.username_input).map_err(|e| e.to_string())
    }

    /// Returns the username currently highlighted by cursor in the users list.
    pub fn highlighted_user(&self) -> Option<&str> {
        self.cursor_user_idx
            .and_then(|idx| self.users.get(idx))
            .map(|s| s.as_str())
    }

    /// Returns the username whose chat conversation is currently open in the right panel.
    pub fn active_chat(&self) -> Option<&str> {
        self.active_chat_user.as_deref()
    }

    /// Moves cursor to next user in list without switching the open chat.
    pub fn select_next_user(&mut self) {
        if self.users.is_empty() {
            self.cursor_user_idx = None;
            return;
        }
        self.cursor_user_idx = Some(match self.cursor_user_idx {
            Some(idx) if idx + 1 < self.users.len() => idx + 1,
            _ => 0,
        });
    }

    /// Moves cursor to previous user in list without switching the open chat.
    pub fn select_prev_user(&mut self) {
        if self.users.is_empty() {
            self.cursor_user_idx = None;
            return;
        }
        self.cursor_user_idx = Some(match self.cursor_user_idx {
            Some(0) | None => self.users.len() - 1,
            Some(idx) => idx - 1,
        });
    }

    /// Opens the currently highlighted user's conversation in the right panel and focuses input.
    pub fn open_selected_chat(&mut self) {
        if let Some(user) = self.highlighted_user().map(|s| s.to_string()) {
            self.active_chat_user = Some(user);
            self.active_panel = ActivePanel::Input;
        }
    }

    /// Focuses on the chat input box. If a chat is already open, focuses it.
    /// Otherwise, opens chat for the currently highlighted user.
    pub fn focus_chat(&mut self) {
        if self.active_chat_user.is_some() {
            self.active_panel = ActivePanel::Input;
        } else {
            self.open_selected_chat();
        }
    }

    /// Processes an incoming message from the server.
    pub fn handle_server_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::RegisterSuccess { username } => {
                self.my_username = username;
                self.screen = Screen::Chat;
                self.is_connecting = false;
                self.connect_error = None;
                self.status_message = Some("Connected to server".to_string());
            }
            ServerMessage::Error { message } => {
                if self.screen == Screen::Connect {
                    self.connect_error = Some(message);
                    self.is_connecting = false;
                } else {
                    self.status_message = Some(format!("Error: {message}"));
                }
            }
            ServerMessage::UserList { users } => {
                self.update_user_list(users);
            }
            ServerMessage::DirectMessage {
                from,
                to,
                content,
                timestamp,
            } => {
                if from == self.my_username {
                    // Sender echo for a message we sent
                    let history = self.chat_histories.entry(to).or_default();
                    if let Some(pending_msg) = history
                        .iter_mut()
                        .find(|m| m.is_me && m.is_pending && m.content == content)
                    {
                        pending_msg.is_pending = false;
                        pending_msg.timestamp = Some(timestamp);
                    } else {
                        history.push(ChatMessage {
                            sender: from,
                            content,
                            is_me: true,
                            is_pending: false,
                            timestamp: Some(timestamp),
                        });
                    }
                } else if to == self.my_username {
                    // Incoming message from `from`
                    self.chat_histories
                        .entry(from.clone())
                        .or_default()
                        .push(ChatMessage {
                            sender: from,
                            content,
                            is_me: false,
                            is_pending: false,
                            timestamp: Some(timestamp),
                        });
                }
            }
            ServerMessage::GroupList { .. } | ServerMessage::GroupMessage { .. } => {
                // Prepared for group features
            }
        }
    }

    /// Updates the user list and wipes chat histories for any users that disconnected.
    pub fn update_user_list(&mut self, new_user_list: Vec<String>) {
        // Filter out ourselves
        let peers: Vec<String> = new_user_list
            .into_iter()
            .filter(|u| u != &self.my_username)
            .collect();

        // Identify disconnected users: previously known users not in the new peers list
        let disconnected_users: Vec<String> = self
            .users
            .iter()
            .filter(|u| !peers.contains(u))
            .cloned()
            .collect();

        // Wipe chat history for disconnected users per SPECS.md
        for disconnected in &disconnected_users {
            self.chat_histories.remove(disconnected);
        }

        // If active chat partner disconnected, close active chat and notify
        if let Some(ref active) = self.active_chat_user
            && disconnected_users.contains(active)
        {
            let disconnected_name = active.clone();
            self.active_chat_user = None;
            self.active_panel = ActivePanel::Users;
            self.status_message = Some(format!(
                "User '{disconnected_name}' disconnected. Chat history wiped."
            ));
        }

        // Remember previously highlighted user to maintain cursor position
        let prev_highlighted = self.highlighted_user().map(|s| s.to_string());

        self.users = peers;

        // Restore or adjust cursor position
        if let Some(prev) = prev_highlighted {
            if let Some(new_idx) = self.users.iter().position(|u| u == &prev) {
                self.cursor_user_idx = Some(new_idx);
            } else if !self.users.is_empty() {
                self.cursor_user_idx = Some(0);
            } else {
                self.cursor_user_idx = None;
            }
        } else if !self.users.is_empty() && self.cursor_user_idx.is_none() {
            self.cursor_user_idx = Some(0);
        } else if self.users.is_empty() {
            self.cursor_user_idx = None;
        }
    }

    /// Sends the current input message as a DM to the currently open active chat.
    /// Appends the message to local history and returns the ClientMessage to send over TCP.
    pub fn send_current_message(&mut self) -> Option<ClientMessage> {
        let recipient = self.active_chat_user.as_ref()?.clone();
        let content = self.input_message.trim().to_string();

        if content.is_empty() {
            return None;
        }

        // Add to local history as pending
        self.chat_histories
            .entry(recipient.clone())
            .or_default()
            .push(ChatMessage {
                sender: self.my_username.clone(),
                content: content.clone(),
                is_me: true,
                is_pending: true,
                timestamp: None,
            });

        self.input_message.clear();

        Some(ClientMessage::DirectMessage { recipient, content })
    }

    /// Handles disconnection event from network layer.
    pub fn on_network_disconnected(&mut self, reason: &str) {
        self.screen = Screen::Connect;
        self.is_connecting = false;
        self.connect_error = Some(format!("Disconnected: {reason}"));
        self.users.clear();
        self.chat_histories.clear();
        self.cursor_user_idx = None;
        self.active_chat_user = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let app = App::new();
        assert_eq!(app.screen, Screen::Connect);
        assert_eq!(app.server_address, "127.0.0.1:8054");
        assert_eq!(app.username_input, "");
        assert_eq!(app.connect_field, ConnectField::Username);
        assert_eq!(app.cursor_user_idx, None);
        assert_eq!(app.active_chat_user, None);
    }

    #[test]
    fn test_validate_connect_input() {
        let mut app = App::new();
        assert!(app.validate_connect_input().is_err());

        app.username_input = "al".to_string();
        assert!(app.validate_connect_input().is_err());

        app.username_input = "Alice".to_string();
        assert!(app.validate_connect_input().is_err());

        app.username_input = "alice".to_string();
        assert!(app.validate_connect_input().is_ok());

        app.server_address = "".to_string();
        assert!(app.validate_connect_input().is_err());
    }

    #[test]
    fn test_registration_success_transition() {
        let mut app = App::new();
        app.is_connecting = true;

        app.handle_server_message(ServerMessage::RegisterSuccess {
            username: "alice".to_string(),
        });

        assert_eq!(app.screen, Screen::Chat);
        assert_eq!(app.my_username, "alice");
        assert!(!app.is_connecting);
        assert!(app.connect_error.is_none());
    }

    #[test]
    fn test_registration_error_display() {
        let mut app = App::new();
        app.is_connecting = true;

        app.handle_server_message(ServerMessage::Error {
            message: "Username 'alice' is already taken".to_string(),
        });

        assert_eq!(app.screen, Screen::Connect);
        assert_eq!(
            app.connect_error,
            Some("Username 'alice' is already taken".to_string())
        );
        assert!(!app.is_connecting);
    }

    #[test]
    fn test_user_list_update_and_wiping_disconnected_user() {
        let mut app = App::new();
        app.screen = Screen::Chat;
        app.my_username = "alice".to_string();

        // 1. Initial user list with bob and charlie
        app.handle_server_message(ServerMessage::UserList {
            users: vec!["alice".to_string(), "bob".to_string(), "charlie".to_string()],
        });

        assert_eq!(app.users, vec!["bob".to_string(), "charlie".to_string()]);
        assert_eq!(app.highlighted_user(), Some("bob"));
        assert_eq!(app.active_chat(), None);

        // 2. Open chat with bob and send a message
        app.open_selected_chat();
        assert_eq!(app.active_chat(), Some("bob"));

        app.input_message = "hello bob".to_string();
        let msg = app.send_current_message();
        assert_eq!(
            msg,
            Some(ClientMessage::DirectMessage {
                recipient: "bob".to_string(),
                content: "hello bob".to_string(),
            })
        );
        assert_eq!(app.chat_histories.get("bob").unwrap().len(), 1);
        assert!(app.chat_histories.get("bob").unwrap()[0].is_pending);
        assert_eq!(app.chat_histories.get("bob").unwrap()[0].timestamp, None);

        // 3. Bob disconnects (new user list only has alice and charlie)
        app.handle_server_message(ServerMessage::UserList {
            users: vec!["alice".to_string(), "charlie".to_string()],
        });

        assert_eq!(app.users, vec!["charlie".to_string()]);
        // Bob's chat history must be wiped!
        assert!(!app.chat_histories.contains_key("bob"));
        // Active chat is now closed since bob disconnected
        assert_eq!(app.active_chat(), None);
        // Cursor points to charlie
        assert_eq!(app.highlighted_user(), Some("charlie"));
    }

    #[test]
    fn test_navigate_without_switching_chat() {
        let mut app = App::new();
        app.screen = Screen::Chat;
        app.my_username = "alice".to_string();

        app.handle_server_message(ServerMessage::UserList {
            users: vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string(),
                "dave".to_string(),
            ],
        });

        // Open chat with bob
        app.cursor_user_idx = Some(0); // bob
        app.open_selected_chat();
        assert_eq!(app.active_chat(), Some("bob"));
        assert_eq!(app.active_panel, ActivePanel::Input);

        // Switch to users panel and navigate around
        app.active_panel = ActivePanel::Users;
        app.select_next_user(); // cursor -> charlie
        assert_eq!(app.highlighted_user(), Some("charlie"));
        // Crucial requirement: right panel active chat MUST still be bob!
        assert_eq!(app.active_chat(), Some("bob"));

        app.select_next_user(); // cursor -> dave
        assert_eq!(app.highlighted_user(), Some("dave"));
        assert_eq!(app.active_chat(), Some("bob"));

        // Only when open_selected_chat() is called does active_chat switch to dave
        app.open_selected_chat();
        assert_eq!(app.active_chat(), Some("dave"));
        assert_eq!(app.active_panel, ActivePanel::Input);
    }

    #[test]
    fn test_dm_receive_appends_to_sender_history() {
        let mut app = App::new();
        app.screen = Screen::Chat;
        app.my_username = "alice".to_string();

        app.handle_server_message(ServerMessage::UserList {
            users: vec!["alice".to_string(), "bob".to_string()],
        });

        // Receive DM from bob
        app.handle_server_message(ServerMessage::DirectMessage {
            from: "bob".to_string(),
            to: "alice".to_string(),
            content: "hey alice!".to_string(),
            timestamp: 1730599839,
        });

        let history = app.chat_histories.get("bob").expect("expected bob history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "hey alice!");
        assert!(!history[0].is_me);
        assert!(!history[0].is_pending);
        assert_eq!(history[0].timestamp, Some(1730599839));
        assert!(history[0].formatted_time().is_some());
    }

    #[test]
    fn test_sender_echo_resolves_pending_message() {
        let mut app = App::new();
        app.screen = Screen::Chat;
        app.my_username = "alice".to_string();
        app.users = vec!["bob".to_string()];
        app.active_chat_user = Some("bob".to_string());

        // Alice sends message
        app.input_message = "msg1".to_string();
        let _ = app.send_current_message();

        let history = app.chat_histories.get("bob").unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].is_pending);
        assert_eq!(history[0].timestamp, None);

        // Echo arrives from server
        app.handle_server_message(ServerMessage::DirectMessage {
            from: "alice".to_string(),
            to: "bob".to_string(),
            content: "msg1".to_string(),
            timestamp: 1730599839,
        });

        let history = app.chat_histories.get("bob").unwrap();
        assert_eq!(history.len(), 1);
        assert!(!history[0].is_pending);
        assert_eq!(history[0].timestamp, Some(1730599839));
    }

    #[test]
    fn test_user_navigation() {
        let mut app = App::new();
        app.screen = Screen::Chat;
        app.my_username = "me".to_string();
        app.users = vec!["u1".to_string(), "u2".to_string(), "u3".to_string()];
        app.cursor_user_idx = Some(0);

        app.select_next_user();
        assert_eq!(app.cursor_user_idx, Some(1));
        assert_eq!(app.highlighted_user(), Some("u2"));

        app.select_next_user();
        assert_eq!(app.cursor_user_idx, Some(2));
        assert_eq!(app.highlighted_user(), Some("u3"));

        app.select_next_user(); // wraps around
        assert_eq!(app.cursor_user_idx, Some(0));
        assert_eq!(app.highlighted_user(), Some("u1"));

        app.select_prev_user(); // wraps backward
        assert_eq!(app.cursor_user_idx, Some(2));
        assert_eq!(app.highlighted_user(), Some("u3"));
    }
}
