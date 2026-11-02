use serde::{Deserialize, Serialize};

/// Messages sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Register { username: String },
    DirectMessage { recipient: String, content: String },
}

/// Messages sent from the server to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    RegisterSuccess { username: String },
    Error { message: String },
    UserList { users: Vec<String> },
    DirectMessage { from: String, to: String, content: String },
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
    fn test_validate_name() {
        assert!(validate_name("alice").is_ok());
        assert!(validate_name("user_123").is_ok());
        assert!(validate_name("abc").is_ok());

        assert!(validate_name("").is_err());
        assert!(validate_name("ab").is_err());
        assert!(validate_name("Alice").is_err()); // uppercase not allowed
        assert!(validate_name("alice-bob").is_err()); // dash not allowed
        assert!(validate_name("alice ").is_err()); // space not allowed
    }

    #[test]
    fn test_message_serde() {
        let msg = ClientMessage::Register {
            username: "alice".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("serialization failed");
        let deserialized: ClientMessage =
            serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(msg, deserialized);

        let srv_msg = ServerMessage::DirectMessage {
            from: "alice".to_string(),
            to: "bob".to_string(),
            content: "hello bob".to_string(),
        };
        let json = serde_json::to_string(&srv_msg).expect("serialization failed");
        let deserialized: ServerMessage =
            serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(srv_msg, deserialized);
    }
}
