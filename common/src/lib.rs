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

/// Default port for Chatatui server and client connections.
pub const DEFAULT_PORT: u16 = 8054;

/// Normalizes a server address input by appending the default port (8054)
/// if no port is specified. Handles IPv4 addresses, hostnames, and IPv6 addresses.
pub fn normalize_server_address(input: &str) -> String {
    let mut trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(stripped) = trimmed.strip_suffix(':') {
        trimmed = stripped;
    }

    // Bracketed IPv6 like "[::1]" or "[::1]:8054"
    if let Some(close_bracket_idx) = trimmed.rfind(']') {
        if trimmed[close_bracket_idx..].contains(':') {
            trimmed.to_string()
        } else {
            format!("{trimmed}:{DEFAULT_PORT}")
        }
    } else if trimmed.chars().filter(|&c| c == ':').count() > 1 {
        // Unbracketed IPv6 like "::1" or "2001:db8::1"
        format!("[{trimmed}]:{DEFAULT_PORT}")
    } else if trimmed.contains(':') {
        // Already contains a port: "127.0.0.1:8054" or "localhost:9000"
        trimmed.to_string()
    } else {
        // Hostname or IPv4 without port: "127.0.0.1" or "localhost"
        format!("{trimmed}:{DEFAULT_PORT}")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_server_address() {
        assert_eq!(normalize_server_address(""), "");
        assert_eq!(normalize_server_address("   "), "");
        assert_eq!(normalize_server_address("127.0.0.1"), "127.0.0.1:8054");
        assert_eq!(normalize_server_address("localhost"), "localhost:8054");
        assert_eq!(normalize_server_address("chat.example.com"), "chat.example.com:8054");
        assert_eq!(normalize_server_address("127.0.0.1:9000"), "127.0.0.1:9000");
        assert_eq!(normalize_server_address("localhost:8054"), "localhost:8054");
        assert_eq!(normalize_server_address("127.0.0.1:"), "127.0.0.1:8054");
        assert_eq!(normalize_server_address("::1"), "[::1]:8054");
        assert_eq!(normalize_server_address("[::1]"), "[::1]:8054");
        assert_eq!(normalize_server_address("[::1]:9000"), "[::1]:9000");
        assert_eq!(normalize_server_address("2001:db8::1"), "[2001:db8::1]:8054");
        assert_eq!(normalize_server_address("[2001:db8::1]:8054"), "[2001:db8::1]:8054");
        assert_eq!(normalize_server_address("  localhost  "), "localhost:8054");
    }
}

