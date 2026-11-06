use serde::{Deserialize, Serialize};

/// Messages sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Register {
        username: String,
    },
    DirectMessage {
        recipient: String,
        content: String,
    },
    GroupMessage {
        group: String,
        content: String,
    },
    CreateGroup {
        #[serde(alias = "name")]
        group: String,
        #[serde(default)]
        password: Option<String>,
    },
    JoinGroup {
        group: String,
        #[serde(default)]
        password: Option<String>,
    },
    LeaveGroup {
        group: String,
    },
    DeleteGroup {
        group: String,
    },
}

/// Metadata describing a chat group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub creator: String,
    pub members: Vec<String>,
    #[serde(default)]
    pub is_protected: bool,
}

/// Messages sent from the server to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    RegisterSuccess {
        username: String,
    },
    Error {
        message: String,
    },
    UserList {
        users: Vec<String>,
    },
    GroupList {
        groups: Vec<GroupInfo>,
    },
    DirectMessage {
        from: String,
        to: String,
        content: String,
        timestamp: u64,
    },
    GroupMessage {
        group: String,
        from: String,
        content: String,
        timestamp: u64,
    },
}

/// Validates a username or group name per SPECS.md:
/// Must be at least 3 characters and consist only of [a-z0-9_].
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.len() < 3 {
        return Err("Name must be at least 3 characters long");
    }
    if !name.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')) {
        return Err("Name must contain only lowercase letters, digits, and underscores ([a-z0-9_])");
    }
    Ok(())
}
