use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use common::{ClientMessage, GroupInfo, ServerMessage, validate_name};

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
    GroupNotFound(String),
    GroupAlreadyExists(String),
    NotGroupMember(String),
    NotGroupCreator(String),
    InvalidPassword(String),
    Unsupported(String),
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
            ServerError::GroupNotFound(g) => write!(f, "Group '{g}' not found"),
            ServerError::GroupAlreadyExists(g) => write!(f, "Group '{g}' already exists"),
            ServerError::NotGroupMember(g) => write!(f, "You are not a member of group '{g}'"),
            ServerError::NotGroupCreator(g) => write!(f, "Only the creator can delete group '{g}'"),
            ServerError::InvalidPassword(g) => write!(f, "Incorrect password for group '{g}'"),
            ServerError::Unsupported(feature) => write!(f, "{feature} is not yet implemented"),
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

/// Represents an in-memory chat group.
#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub creator: String,
    pub members: HashSet<String>,
    pub password: Option<String>,
}

/// In-memory state of the chat server.
/// Decoupled from transport (TCP) for testability.
pub struct ServerState {
    clients: HashMap<ClientId, ClientSession>,
    users: HashMap<String, ClientId>,
    used_usernames: HashSet<String>,
    groups: HashMap<String, Group>,
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
            used_usernames: HashSet::new(),
            groups: HashMap::new(),
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

            // 1. Delete groups created by this user per SPECS.md
            self.groups.retain(|_, grp| grp.creator != username);

            // 2. Remove user from any other groups they joined and send leave notice
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            for (grp_name, grp) in self.groups.iter_mut() {
                if grp.members.remove(&username) {
                    let notice = ServerMessage::GroupMessage {
                        group: grp_name.clone(),
                        from: "system".to_string(),
                        content: format!("@{username} left the group"),
                        timestamp: now,
                    };
                    for member in &grp.members {
                        if let Some(&m_id) = self.users.get(member)
                            && let Some(m_session) = self.clients.get(&m_id)
                        {
                            let _ = m_session.tx.send(notice.clone());
                        }
                    }
                }
            }

            // Broadcast updated user list to all remaining registered clients
            let user_list = self.get_user_list();
            self.broadcast_to_registered(ServerMessage::UserList { users: user_list });

            // Broadcast updated group list to all remaining registered clients
            let group_list = self.get_group_list();
            self.broadcast_to_registered(ServerMessage::GroupList { groups: group_list });

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
            ClientMessage::CreateGroup { group, password } => {
                self.handle_create_group(client_id, group, password)
            }
            ClientMessage::JoinGroup { group, password } => {
                self.handle_join_group(client_id, group, password)
            }
            ClientMessage::LeaveGroup { group } => self.handle_leave_group(client_id, group),
            ClientMessage::DeleteGroup { group } => self.handle_delete_group(client_id, group),
            ClientMessage::GroupMessage { group, content } => {
                self.handle_group_message(client_id, group, content)
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

        if self.used_usernames.contains(&username) {
            return Err(ServerError::UsernameTaken(username));
        }

        self.used_usernames.insert(username.clone());
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

        let groups = self.get_group_list();
        if !groups.is_empty() {
            self.send_to(client_id, ServerMessage::GroupList { groups });
        }

        Ok(())
    }

    /// Handles routing direct messages between registered users with timestamp and sender echo.
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

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let srv_msg = ServerMessage::DirectMessage {
            from: sender_username,
            to: recipient,
            content,
            timestamp,
        };

        self.send_to(recipient_id, srv_msg.clone());
        if recipient_id != client_id {
            self.send_to(client_id, srv_msg);
        }

        Ok(())
    }

    /// Handles group creation. Creator is the sole initial member (R8).
    pub fn handle_create_group(
        &mut self,
        client_id: ClientId,
        group_name: String,
        password: Option<String>,
    ) -> Result<(), ServerError> {
        let username = self.assert_registered(client_id)?;
        validate_name(&group_name).map_err(|e| ServerError::InvalidName(e.to_string()))?;

        if self.groups.contains_key(&group_name) {
            return Err(ServerError::GroupAlreadyExists(group_name));
        }

        let mut members = HashSet::new();
        members.insert(username.clone());

        let password = password.and_then(|p| {
            let trimmed = p.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        self.groups.insert(
            group_name.clone(),
            Group {
                name: group_name,
                creator: username,
                members,
                password,
            },
        );

        let group_list = self.get_group_list();
        self.broadcast_to_registered(ServerMessage::GroupList { groups: group_list });

        Ok(())
    }

    /// Handles joining a group chat (R10) and sends a join notice to all group members.
    pub fn handle_join_group(
        &mut self,
        client_id: ClientId,
        group_name: String,
        password: Option<String>,
    ) -> Result<(), ServerError> {
        let username = self.assert_registered(client_id)?;

        let (was_new, members) = {
            let group = self
                .groups
                .get_mut(&group_name)
                .ok_or_else(|| ServerError::GroupNotFound(group_name.clone()))?;

            if !group.members.contains(&username) {
                if let Some(ref required_pw) = group.password {
                    let provided_pw = password.as_deref().unwrap_or("").trim();
                    if provided_pw != required_pw {
                        return Err(ServerError::InvalidPassword(group_name));
                    }
                }
            }

            let was_new = group.members.insert(username.clone());
            let members: Vec<String> = group.members.iter().cloned().collect();
            (was_new, members)
        };

        if was_new {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let notice = ServerMessage::GroupMessage {
                group: group_name.clone(),
                from: "system".to_string(),
                content: format!("@{username} joined the group"),
                timestamp,
            };

            for member in members {
                if let Some(&m_id) = self.users.get(&member) {
                    self.send_to(m_id, notice.clone());
                }
            }

            let group_list = self.get_group_list();
            self.broadcast_to_registered(ServerMessage::GroupList { groups: group_list });
        }

        Ok(())
    }

    /// Handles leaving a group chat and sends a leave notice to remaining group members.
    pub fn handle_leave_group(
        &mut self,
        client_id: ClientId,
        group_name: String,
    ) -> Result<(), ServerError> {
        let username = self.assert_registered(client_id)?;

        let (removed, members, is_empty) = {
            let group = self
                .groups
                .get_mut(&group_name)
                .ok_or_else(|| ServerError::GroupNotFound(group_name.clone()))?;

            let removed = group.members.remove(&username);
            let members: Vec<String> = group.members.iter().cloned().collect();
            let is_empty = group.members.is_empty();
            (removed, members, is_empty)
        };

        if !removed {
            return Err(ServerError::NotGroupMember(group_name));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let notice = ServerMessage::GroupMessage {
            group: group_name.clone(),
            from: "system".to_string(),
            content: format!("@{username} left the group"),
            timestamp,
        };

        for member in members {
            if let Some(&m_id) = self.users.get(&member) {
                self.send_to(m_id, notice.clone());
            }
        }

        if is_empty {
            self.groups.remove(&group_name);
        }

        let group_list = self.get_group_list();
        self.broadcast_to_registered(ServerMessage::GroupList { groups: group_list });

        Ok(())
    }

    /// Handles deleting a group. Only the group creator can delete it (SPECS.md).
    pub fn handle_delete_group(
        &mut self,
        client_id: ClientId,
        group_name: String,
    ) -> Result<(), ServerError> {
        let username = self.assert_registered(client_id)?;

        let group = self
            .groups
            .get(&group_name)
            .ok_or_else(|| ServerError::GroupNotFound(group_name.clone()))?;

        if group.creator != username {
            return Err(ServerError::NotGroupCreator(group_name));
        }

        self.groups.remove(&group_name);

        let group_list = self.get_group_list();
        self.broadcast_to_registered(ServerMessage::GroupList { groups: group_list });

        Ok(())
    }

    /// Handles broadcasting a text message to all members of the group (R11).
    pub fn handle_group_message(
        &mut self,
        client_id: ClientId,
        group_name: String,
        content: String,
    ) -> Result<(), ServerError> {
        let username = self.assert_registered(client_id)?;

        let group = self
            .groups
            .get(&group_name)
            .ok_or_else(|| ServerError::GroupNotFound(group_name.clone()))?;

        if !group.members.contains(&username) {
            return Err(ServerError::NotGroupMember(group_name));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let ai_prompt = content.strip_prefix("@ai").map(|p| p.trim().to_string());
        let group_for_ai = group_name.clone();

        let srv_msg = ServerMessage::GroupMessage {
            group: group_name,
            from: username,
            content,
            timestamp,
        };

        for member in &group.members {
            if let Some(&m_id) = self.users.get(member) {
                self.send_to(m_id, srv_msg.clone());
            }
        }

        if let Some(prompt) = ai_prompt {
            // 1. grab every member's queue while we still have self
            let queues: Vec<_> = group
                .members
                .iter()
                .filter_map(|m| self.users.get(m))
                .filter_map(|id| self.clients.get(id))
                .map(|session| session.tx.clone())
                .collect();

            // 2. background task: wait for the answer, then post it
            tokio::spawn(async move {
                let answer = ask_ai(&prompt).await;
                let msg = ServerMessage::GroupMessage {
                    group: group_for_ai,
                    from: "ai".to_string(),
                    content: answer,
                    timestamp,
                };
                for q in queues {
                    let _ = q.send(msg.clone());
                }
            });
        }

        Ok(())
    }

    /// Returns a list of all existing groups with member lists (R9).
    pub fn get_group_list(&self) -> Vec<GroupInfo> {
        let mut list: Vec<GroupInfo> = self
            .groups
            .values()
            .map(|g| {
                let mut members: Vec<String> = g.members.iter().cloned().collect();
                members.sort();
                GroupInfo {
                    name: g.name.clone(),
                    creator: g.creator.clone(),
                    members,
                    is_protected: g.password.is_some(),
                }
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
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

async fn ask_ai(prompt: &str) -> String {
    let busy = "AI is busy, try again".to_string();

    // your key
    let Ok(key) = std::env::var("GEMINI_API_KEY") else { return busy };

    // -d '{...}'
    let body = serde_json::json!({
        "model": "gemini-3.5-flash-lite",
        "input": prompt,
    });

    // curl -X POST ... -H ...
    let Ok(response) = reqwest::Client::new()
        .post("https://generativelanguage.googleapis.com/v1beta/interactions")
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .await
    else { return busy };

    // read the reply as JSON
    let Ok(json) = response.json::<serde_json::Value>().await else { return busy };

    // steps -> the model_output step -> content[0] -> text
    json["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|s| s["type"] == "model_output"))
        .and_then(|step| step["content"][0]["text"].as_str())
        .map(|text| text.to_string())
        .unwrap_or(busy)
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

    #[test]
    fn test_disconnected_username_cannot_be_reused() {
        let mut state = ServerState::new();
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
        let c1 = state.connect_client(tx1);

        // Client 1 registers
        assert!(state.handle_register(c1, "alice".to_string()).is_ok());

        // Client 1 disconnects
        let disconnected = state.disconnect_client(c1);
        assert_eq!(disconnected, Some("alice".to_string()));
        assert!(!state.is_registered("alice"));

        // Client 2 attempts to register with same username 'alice' -> rejected
        let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();
        let c2 = state.connect_client(tx2);
        let res = state.handle_register(c2, "alice".to_string());
        assert_eq!(res, Err(ServerError::UsernameTaken("alice".to_string())));
    }

    #[test]
    fn test_password_protected_group_lifecycle() {
        let mut state = ServerState::new();
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
        let c1 = state.connect_client(tx1);
        assert!(state.handle_register(c1, "alice".to_string()).is_ok());

        let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();
        let c2 = state.connect_client(tx2);
        assert!(state.handle_register(c2, "bob".to_string()).is_ok());

        // Alice creates protected group 'secret_room' with password 'pass123'
        assert!(state.handle_create_group(c1, "secret_room".to_string(), Some("pass123".to_string())).is_ok());

        // Check group list reflects protection
        let groups = state.get_group_list();
        let grp = groups.iter().find(|g| g.name == "secret_room").unwrap();
        assert!(grp.is_protected);

        // Bob tries to join without password -> error
        let err1 = state.handle_join_group(c2, "secret_room".to_string(), None);
        assert_eq!(err1, Err(ServerError::InvalidPassword("secret_room".to_string())));

        // Bob tries to join with wrong password -> error
        let err2 = state.handle_join_group(c2, "secret_room".to_string(), Some("wrong".to_string()));
        assert_eq!(err2, Err(ServerError::InvalidPassword("secret_room".to_string())));

        // Bob joins with correct password -> success
        let ok = state.handle_join_group(c2, "secret_room".to_string(), Some("pass123".to_string()));
        assert!(ok.is_ok());
    }
}
