use std::collections::{HashMap, HashSet};

use common::{validate_name, ClientMessage, GroupInfo, ServerMessage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
    pub is_me: bool,
    pub is_pending: bool,
    pub timestamp: Option<u64>,
    pub is_mention: bool,
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
    Groups,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateGroupField {
    Name,
    Password,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    CreateGroup {
        name_input: String,
        password_input: String,
        active_field: CreateGroupField,
        error: Option<String>,
    },
    EnterGroupPassword {
        group_name: String,
        password_input: String,
        error: Option<String>,
    },
    ConfirmLeaveGroup {
        group_name: String,
    },
    ConfirmDeleteGroup {
        group_name: String,
    },
    ViewMembers {
        group_name: String,
        members: Vec<String>,
    },
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

    pub groups: Vec<GroupInfo>,
    /// The group index currently highlighted in the ordered group list.
    pub cursor_group_idx: Option<usize>,
    /// The group whose conversation is currently open in the Right Panel.
    pub active_chat_group: Option<String>,

    pub modal: Option<Modal>,
    pub active_panel: ActivePanel,
    pub chat_histories: HashMap<String, Vec<ChatMessage>>,
    pub input_message: String,
    pub status_message: Option<String>,

    /// Unread indicators for chats
    pub unread_messages: HashSet<String>,
    pub unread_mentions: HashSet<String>,

    /// Autocomplete state for @mentions
    pub autocomplete_active: bool,
    pub autocomplete_dismissed: bool,
    pub autocomplete_candidates: Vec<String>,
    pub autocomplete_selected: usize,
}

/// Returns whether the message content mentions the given username with '@<username>'.
pub fn is_mentioned(content: &str, username: &str) -> bool {
    if username.is_empty() {
        return false;
    }
    let target = format!("@{username}");
    let mut rest = content;
    while let Some(pos) = rest.find(&target) {
        let after = &rest[pos + target.len()..];
        let is_word_end = after.chars().next().map_or(true, |c| !c.is_alphanumeric() && c != '_');
        if is_word_end {
            return true;
        }
        rest = after;
    }
    false
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

            groups: Vec::new(),
            cursor_group_idx: None,
            active_chat_group: None,

            modal: None,
            active_panel: ActivePanel::Users,
            chat_histories: HashMap::new(),
            input_message: String::new(),
            status_message: None,

            unread_messages: HashSet::new(),
            unread_mentions: HashSet::new(),

            autocomplete_active: false,
            autocomplete_dismissed: false,
            autocomplete_candidates: Vec::new(),
            autocomplete_selected: 0,
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

    /// Returns the username whose chat conversation is currently open, or the group name.
    pub fn active_chat(&self) -> Option<&str> {
        self.active_chat_group
            .as_deref()
            .or(self.active_chat_user.as_deref())
    }

    pub fn is_group_chat(&self) -> bool {
        self.active_chat_group.is_some()
    }

    pub fn group_history_key(group: &str) -> String {
        format!("group:{group}")
    }

    /// Returns the ordered list of groups per SPECS.md:
    /// 1. Groups created by me
    /// 2. Groups joined by me
    /// 3. Other groups
    pub fn ordered_groups(&self) -> Vec<GroupInfo> {
        let mut created = Vec::new();
        let mut joined = Vec::new();
        let mut others = Vec::new();

        for g in &self.groups {
            if g.creator == self.my_username {
                created.push(g.clone());
            } else if g.members.contains(&self.my_username) {
                joined.push(g.clone());
            } else {
                others.push(g.clone());
            }
        }

        created.sort_by(|a, b| a.name.cmp(&b.name));
        joined.sort_by(|a, b| a.name.cmp(&b.name));
        others.sort_by(|a, b| a.name.cmp(&b.name));

        let mut all = created;
        all.extend(joined);
        all.extend(others);
        all
    }

    /// Returns the group currently highlighted by cursor in the ordered groups list.
    pub fn highlighted_group(&self) -> Option<GroupInfo> {
        let list = self.ordered_groups();
        self.cursor_group_idx.and_then(|idx| list.get(idx).cloned())
    }

    /// Moves cursor to next group in list without switching the open chat.
    pub fn select_next_group(&mut self) {
        let len = self.groups.len();
        if len == 0 {
            self.cursor_group_idx = None;
            return;
        }
        self.cursor_group_idx = Some(match self.cursor_group_idx {
            Some(idx) if idx + 1 < len => idx + 1,
            _ => 0,
        });
    }

    /// Moves cursor to previous group in list without switching the open chat.
    pub fn select_prev_group(&mut self) {
        let len = self.groups.len();
        if len == 0 {
            self.cursor_group_idx = None;
            return;
        }
        self.cursor_group_idx = Some(match self.cursor_group_idx {
            Some(0) | None => len - 1,
            Some(idx) => idx - 1,
        });
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
            if user == self.my_username {
                self.status_message = Some("Cannot DM yourself".to_string());
                return;
            }
            self.unread_messages.remove(&user);
            self.active_chat_user = Some(user);
            self.active_chat_group = None;
            self.active_panel = ActivePanel::Input;
        }
    }

    /// Opens the currently highlighted group's conversation in the right panel if joined.
    pub fn open_selected_group_chat(&mut self) {
        if let Some(grp) = self.highlighted_group() {
            if grp.members.contains(&self.my_username) {
                self.unread_mentions.remove(&grp.name);
                self.unread_messages.remove(&grp.name);
                self.active_chat_group = Some(grp.name);
                self.active_chat_user = None;
                self.active_panel = ActivePanel::Input;
            } else {
                self.status_message = Some(format!("Join group '{}' first", grp.name));
            }
        }
    }

    /// Joins a group and immediately opens its conversation in the right panel and focuses input.
    pub fn join_and_open_group(&mut self, group_name: &str) {
        if let Some(grp) = self.groups.iter_mut().find(|g| g.name == group_name) {
            if !grp.members.contains(&self.my_username) {
                grp.members.push(self.my_username.clone());
            }
        }
        self.unread_mentions.remove(group_name);
        self.unread_messages.remove(group_name);
        self.active_chat_group = Some(group_name.to_string());
        self.active_chat_user = None;
        self.active_panel = ActivePanel::Input;
        self.status_message = Some(format!("Joined group '#{group_name}'"));

        // Adjust cursor position to the group's new index in ordered_groups
        let ordered = self.ordered_groups();
        if let Some(idx) = ordered.iter().position(|g| g.name == group_name) {
            self.cursor_group_idx = Some(idx);
        }
    }

    /// Focuses on the chat input box. If a chat is already open, focuses it.
    /// Otherwise, opens chat for the currently highlighted user or group.
    pub fn focus_chat(&mut self) {
        if self.active_chat_user.is_some() || self.active_chat_group.is_some() {
            self.active_panel = ActivePanel::Input;
        } else if self.active_panel == ActivePanel::Groups {
            self.open_selected_group_chat();
        } else {
            self.open_selected_chat();
        }
    }

    /// Returns a slice of chat messages for the currently open conversation.
    pub fn current_chat_history(&self) -> &[ChatMessage] {
        if let Some(ref g) = self.active_chat_group {
            let key = Self::group_history_key(g);
            self.chat_histories
                .get(&key)
                .map(|v| v.as_slice())
                .unwrap_or(&[])
        } else if let Some(ref u) = self.active_chat_user {
            self.chat_histories
                .get(u)
                .map(|v| v.as_slice())
                .unwrap_or(&[])
        } else {
            &[]
        }
    }

    /// Updates autocomplete state based on current input_message.
    pub fn update_autocomplete(&mut self) {
        if self.autocomplete_dismissed {
            return;
        }

        // Find the last '@'
        if let Some(at_idx) = self.input_message.rfind('@') {
            // Check that preceding char is start of string or whitespace
            let is_at_start = at_idx == 0
                || self.input_message[..at_idx]
                    .chars()
                    .last()
                    .map_or(false, |c| c.is_whitespace());

            if is_at_start {
                let query = &self.input_message[at_idx + 1..];
                // Autocomplete only if query doesn't contain spaces
                if !query.contains(char::is_whitespace) {
                    let mut candidates: Vec<String> = Vec::new();

                    // If active group, suggest group members first, else all users
                    if let Some(ref grp_name) = self.active_chat_group {
                        if let Some(grp) = self.groups.iter().find(|g| &g.name == grp_name) {
                            for member in &grp.members {
                                if member != &self.my_username
                                    && member.to_lowercase().starts_with(&query.to_lowercase())
                                {
                                    candidates.push(member.clone());
                                }
                            }
                        }
                    } else {
                        for user in &self.users {
                            if user != &self.my_username
                                && user.to_lowercase().starts_with(&query.to_lowercase())
                            {
                                candidates.push(user.clone());
                            }
                        }
                    }

                    if !candidates.is_empty() {
                        candidates.sort();
                        self.autocomplete_candidates = candidates;
                        self.autocomplete_active = true;
                        self.autocomplete_selected = 0;
                        return;
                    }
                }
            }
        }

        self.autocomplete_active = false;
        self.autocomplete_candidates.clear();
        self.autocomplete_selected = 0;
    }

    /// Selects the next autocomplete candidate.
    pub fn autocomplete_next(&mut self) {
        if !self.autocomplete_candidates.is_empty() {
            self.autocomplete_selected =
                (self.autocomplete_selected + 1) % self.autocomplete_candidates.len();
        }
    }

    /// Selects the previous autocomplete candidate.
    pub fn autocomplete_prev(&mut self) {
        if !self.autocomplete_candidates.is_empty() {
            self.autocomplete_selected = (self.autocomplete_selected
                + self.autocomplete_candidates.len()
                - 1)
                % self.autocomplete_candidates.len();
        }
    }

    /// Applies the currently selected candidate and closes autocomplete.
    pub fn apply_autocomplete(&mut self) -> bool {
        if !self.autocomplete_active || self.autocomplete_candidates.is_empty() {
            return false;
        }

        if let Some(at_idx) = self.input_message.rfind('@') {
            let selected_user = self.autocomplete_candidates[self.autocomplete_selected].clone();
            self.input_message.truncate(at_idx);
            self.input_message.push('@');
            self.input_message.push_str(&selected_user);
            self.input_message.push(' ');
            self.autocomplete_active = false;
            self.autocomplete_dismissed = true;
            self.autocomplete_candidates.clear();
            self.autocomplete_selected = 0;
            return true;
        }

        false
    }

    /// Dismisses autocomplete popup without modifying text.
    pub fn dismiss_autocomplete(&mut self) {
        self.autocomplete_active = false;
        self.autocomplete_dismissed = true;
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
                } else if let Some(Modal::EnterGroupPassword { ref mut error, .. }) = self.modal {
                    *error = Some(message);
                } else if let Some(Modal::CreateGroup { ref mut error, .. }) = self.modal {
                    *error = Some(message);
                } else {
                    self.status_message = Some(format!("Error: {message}"));
                }
            }
            ServerMessage::UserList { users } => {
                self.update_user_list(users);
            }
            ServerMessage::GroupList { groups } => {
                self.update_group_list(groups);
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
                            is_mention: false,
                        });
                    }
                } else if to == self.my_username {
                    // Incoming message from `from`
                    if self.active_chat_user.as_deref() != Some(&from) {
                        self.unread_messages.insert(from.clone());
                    }
                    self.chat_histories
                        .entry(from.clone())
                        .or_default()
                        .push(ChatMessage {
                            sender: from,
                            content,
                            is_me: false,
                            is_pending: false,
                            timestamp: Some(timestamp),
                            is_mention: false,
                        });
                }
            }
            ServerMessage::GroupMessage {
                group,
                from,
                content,
                timestamp,
            } => {
                let key = Self::group_history_key(&group);
                let history = self.chat_histories.entry(key).or_default();
                let mentions_me = is_mentioned(&content, &self.my_username);

                if from == self.my_username {
                    // Sender echo for a group message we sent
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
                            is_mention: false,
                        });
                    }
                } else {
                    let is_current_group = self.active_chat_group.as_deref() == Some(&group);
                    if mentions_me {
                        // Ring terminal bell
                        print!("\x07");
                        let _ = std::io::Write::flush(&mut std::io::stdout());
                        self.status_message = Some(format!("Mentioned by @{from} in #{group}"));
                        if !is_current_group {
                            self.unread_mentions.insert(group.clone());
                        }
                    } else if !is_current_group {
                        self.unread_messages.insert(group.clone());
                    }

                    history.push(ChatMessage {
                        sender: from,
                        content,
                        is_me: false,
                        is_pending: false,
                        timestamp: Some(timestamp),
                        is_mention: mentions_me,
                    });
                }
            }
        }
    }

    /// Cleans up state when leaving a group:
    /// Always wipes the group's chat history.
    /// If the group was open in the active chat, resets active conversation and switches to Groups panel.
    pub fn leave_group_cleanup(&mut self, group_name: &str) {
        self.chat_histories
            .remove(&Self::group_history_key(group_name));
        self.unread_mentions.remove(group_name);
        self.unread_messages.remove(group_name);
        if self.active_chat_group.as_deref() == Some(group_name) {
            self.active_chat_group = None;
            self.active_panel = ActivePanel::Groups;
            self.status_message =
                Some(format!("Left group '#{group_name}'. Chat history wiped."));
        }
    }

    /// Updates the group list and wipes chat histories for any groups that were deleted.
    pub fn update_group_list(&mut self, new_groups: Vec<GroupInfo>) {
        let new_names: Vec<String> = new_groups.iter().map(|g| g.name.clone()).collect();

        // Identify deleted groups
        let deleted_groups: Vec<String> = self
            .groups
            .iter()
            .filter(|g| !new_names.contains(&g.name))
            .map(|g| g.name.clone())
            .collect();

        // Wipe history for deleted groups per SPECS.md
        for deleted in &deleted_groups {
            self.chat_histories
                .remove(&Self::group_history_key(deleted));
        }

        // If currently active group was deleted, reset active chat
        if let Some(ref active_grp) = self.active_chat_group
            && deleted_groups.contains(active_grp)
        {
            let deleted_name = active_grp.clone();
            self.active_chat_group = None;
            self.active_panel = ActivePanel::Groups;
            self.status_message = Some(format!(
                "Group '{deleted_name}' was deleted. Chat history wiped."
            ));
        } else if let Some(ref active_grp) = self.active_chat_group {
            // If we are no longer a member of the active group (e.g. left group)
            let is_member = new_groups
                .iter()
                .any(|g| &g.name == active_grp && g.members.contains(&self.my_username));
            if !is_member {
                let left_name = active_grp.clone();
                self.chat_histories
                    .remove(&Self::group_history_key(&left_name));
                self.active_chat_group = None;
                self.active_panel = ActivePanel::Groups;
                self.status_message =
                    Some(format!("Left group '#{left_name}'. Chat history wiped."));
            }
        }

        let prev_highlighted = self.highlighted_group().map(|g| g.name);

        self.groups = new_groups;

        // If we were waiting in EnterGroupPassword for a group that we have now successfully joined, close modal and open chat
        if let Some(Modal::EnterGroupPassword { ref group_name, .. }) = self.modal {
            if let Some(grp) = self.groups.iter().find(|g| &g.name == group_name) {
                if grp.members.contains(&self.my_username) {
                    let name = group_name.clone();
                    self.modal = None;
                    self.join_and_open_group(&name);
                }
            }
        }

        // Restore cursor position if possible
        let ordered = self.ordered_groups();
        if let Some(prev) = prev_highlighted {
            if let Some(new_idx) = ordered.iter().position(|g| g.name == prev) {
                self.cursor_group_idx = Some(new_idx);
            } else if !ordered.is_empty() {
                self.cursor_group_idx = Some(0);
            } else {
                self.cursor_group_idx = None;
            }
        } else if !ordered.is_empty() && self.cursor_group_idx.is_none() {
            self.cursor_group_idx = Some(0);
        } else if ordered.is_empty() {
            self.cursor_group_idx = None;
        }
    }

    /// Updates the user list and wipes chat histories for any users that disconnected.
    pub fn update_user_list(&mut self, new_user_list: Vec<String>) {
        let mut peers: Vec<String> = new_user_list
            .iter()
            .filter(|u| *u != &self.my_username)
            .cloned()
            .collect();
        peers.sort();

        // Identify disconnected users: previously known users (excluding me) not in new peers list
        let disconnected_users: Vec<String> = self
            .users
            .iter()
            .filter(|u| *u != &self.my_username && !peers.contains(u))
            .cloned()
            .collect();

        // Wipe chat history for disconnected users per SPECS.md
        for disconnected in &disconnected_users {
            self.chat_histories.remove(disconnected);
            self.unread_messages.remove(disconnected);
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

        let mut full_list = Vec::new();
        if !self.my_username.is_empty() && new_user_list.contains(&self.my_username) {
            full_list.push(self.my_username.clone());
        }
        full_list.extend(peers);
        self.users = full_list;

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

    /// Sends the current input message to the currently open active chat (DM or Group).
    /// Appends the message to local history and returns the ClientMessage to send over TCP.
    pub fn send_current_message(&mut self) -> Option<ClientMessage> {
        let content = self.input_message.trim().to_string();
        if content.is_empty() {
            return None;
        }

        self.autocomplete_active = false;
        self.autocomplete_candidates.clear();
        self.autocomplete_selected = 0;

        if let Some(ref group) = self.active_chat_group {
            let key = Self::group_history_key(group);
            self.chat_histories
                .entry(key)
                .or_default()
                .push(ChatMessage {
                    sender: self.my_username.clone(),
                    content: content.clone(),
                    is_me: true,
                    is_pending: true,
                    timestamp: None,
                    is_mention: false,
                });

            let grp_name = group.clone();
            self.input_message.clear();
            Some(ClientMessage::GroupMessage {
                group: grp_name,
                content,
            })
        } else if let Some(ref recipient) = self.active_chat_user {
            self.chat_histories
                .entry(recipient.clone())
                .or_default()
                .push(ChatMessage {
                    sender: self.my_username.clone(),
                    content: content.clone(),
                    is_me: true,
                    is_pending: true,
                    timestamp: None,
                    is_mention: false,
                });

            let rec = recipient.clone();
            self.input_message.clear();
            Some(ClientMessage::DirectMessage {
                recipient: rec,
                content,
            })
        } else {
            None
        }
    }

    /// Handles disconnection event from network layer.
    pub fn on_network_disconnected(&mut self, reason: &str) {
        self.screen = Screen::Connect;
        self.is_connecting = false;
        self.connect_error = Some(format!("Disconnected: {reason}"));
        self.users.clear();
        self.groups.clear();
        self.chat_histories.clear();
        self.cursor_user_idx = None;
        self.cursor_group_idx = None;
        self.active_chat_user = None;
        self.active_chat_group = None;
        self.modal = None;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_list_includes_self_first_and_disables_self_dm() {
        let mut app = App::new();
        app.my_username = "alice".to_string();

        app.update_user_list(vec!["charlie".to_string(), "alice".to_string(), "bob".to_string()]);

        // Self should be first, followed by alphabetically sorted peers
        assert_eq!(app.users, vec!["alice", "bob", "charlie"]);
        assert_eq!(app.cursor_user_idx, Some(0));

        // Attempting to DM oneself
        app.open_selected_chat();
        assert_eq!(app.active_chat_user, None);
        assert_eq!(app.status_message, Some("Cannot DM yourself".to_string()));

        // Move to bob and open DM
        app.select_next_user();
        assert_eq!(app.cursor_user_idx, Some(1));
        assert_eq!(app.highlighted_user(), Some("bob"));
        app.open_selected_chat();
        assert_eq!(app.active_chat_user, Some("bob".to_string()));
    }

    #[test]
    fn test_leaving_group_resets_chat_and_clears_history() {
        let mut app = App::new();
        app.my_username = "alice".to_string();
        app.active_chat_group = Some("rustacean".to_string());
        app.active_panel = ActivePanel::Input;

        let key = App::group_history_key("rustacean");
        app.chat_histories.insert(
            key.clone(),
            vec![ChatMessage {
                sender: "alice".to_string(),
                content: "hello world".to_string(),
                is_me: true,
                is_pending: false,
                timestamp: Some(1700000000),
                is_mention: false,
            }],
        );

        assert_eq!(app.chat_histories.get(&key).unwrap().len(), 1);

        // Leave group
        app.leave_group_cleanup("rustacean");

        assert_eq!(app.active_chat_group, None);
        assert_eq!(app.active_panel, ActivePanel::Groups);
        assert!(!app.chat_histories.contains_key(&key));
    }

    #[test]
    fn test_ordered_groups_roles() {
        let mut app = App::new();
        app.my_username = "alice".to_string();

        app.groups = vec![
            GroupInfo {
                name: "other_group".to_string(),
                creator: "bob".to_string(),
                members: vec!["bob".to_string()],
                is_protected: false,
            },
            GroupInfo {
                name: "joined_group".to_string(),
                creator: "charlie".to_string(),
                members: vec!["alice".to_string(), "charlie".to_string()],
                is_protected: true,
            },
            GroupInfo {
                name: "created_group".to_string(),
                creator: "alice".to_string(),
                members: vec!["alice".to_string()],
                is_protected: false,
            },
        ];

        let ordered = app.ordered_groups();
        assert_eq!(ordered[0].name, "created_group");
        assert_eq!(ordered[1].name, "joined_group");
        assert_eq!(ordered[2].name, "other_group");
    }

    #[test]
    fn test_join_and_open_group() {
        let mut app = App::new();
        app.my_username = "alice".to_string();
        app.groups = vec![GroupInfo {
            name: "group_by_two".to_string(),
            creator: "two".to_string(),
            members: vec!["two".to_string()],
            is_protected: false,
        }];

        assert_eq!(app.active_chat_group, None);
        assert_ne!(app.active_panel, ActivePanel::Input);

        app.join_and_open_group("group_by_two");

        assert_eq!(app.active_chat_group, Some("group_by_two".to_string()));
        assert_eq!(app.active_panel, ActivePanel::Input);
        assert!(app.groups[0].members.contains(&"alice".to_string()));
    }

    #[test]
    fn test_is_mentioned() {
        assert!(is_mentioned("hey @alice how are you?", "alice"));
        assert!(is_mentioned("@alice!", "alice"));
        assert!(is_mentioned("hello @alice_smith and @alice", "alice"));
        // Does not match longer username
        assert!(!is_mentioned("hello @alice_smith", "alice"));
        // Does not match non-mention
        assert!(!is_mentioned("alice without at", "alice"));
        // Empty username
        assert!(!is_mentioned("hey @alice", ""));
    }

    #[test]
    fn test_autocomplete_flow() {
        let mut app = App::new();
        app.my_username = "alice".to_string();
        app.groups = vec![GroupInfo {
            name: "rust".to_string(),
            creator: "bob".to_string(),
            members: vec!["alice".to_string(), "bob".to_string(), "bobby".to_string(), "charlie".to_string()],
            is_protected: false,
        }];
        app.active_chat_group = Some("rust".to_string());

        // Type '@'
        app.input_message = "hello @".to_string();
        app.update_autocomplete();
        assert!(app.autocomplete_active);
        // Excludes 'alice' (me)
        assert_eq!(app.autocomplete_candidates, vec!["bob", "bobby", "charlie"]);

        // Type 'b'
        app.input_message = "hello @b".to_string();
        app.update_autocomplete();
        assert!(app.autocomplete_active);
        assert_eq!(app.autocomplete_candidates, vec!["bob", "bobby"]);

        // Navigate
        app.autocomplete_next();
        assert_eq!(app.autocomplete_selected, 1);

        // Apply autocomplete (selects 'bobby')
        assert!(app.apply_autocomplete());
        assert_eq!(app.input_message, "hello @bobby ");
        assert!(!app.autocomplete_active);

        // Dismiss flow: typing '@' then dismissing
        app.autocomplete_dismissed = false;
        app.input_message = "test @".to_string();
        app.update_autocomplete();
        assert!(app.autocomplete_active);
        app.dismiss_autocomplete();
        assert!(!app.autocomplete_active);
    }

    #[test]
    fn test_unread_mentions_and_regular_unreads() {
        let mut app = App::new();
        app.my_username = "alice".to_string();
        app.screen = Screen::Chat;

        // Group message without mention when not active
        app.handle_server_message(ServerMessage::GroupMessage {
            group: "rust".to_string(),
            from: "bob".to_string(),
            content: "hello world".to_string(),
            timestamp: 1000,
        });
        assert!(app.unread_messages.contains("rust"));
        assert!(!app.unread_mentions.contains("rust"));

        // Group message WITH mention when not active
        app.handle_server_message(ServerMessage::GroupMessage {
            group: "rust".to_string(),
            from: "bob".to_string(),
            content: "hello @alice please review".to_string(),
            timestamp: 1001,
        });
        assert!(app.unread_mentions.contains("rust"));

        // Opening chat clears mentions and unreads
        app.groups = vec![GroupInfo {
            name: "rust".to_string(),
            creator: "bob".to_string(),
            members: vec!["alice".to_string(), "bob".to_string()],
            is_protected: false,
        }];
        app.cursor_group_idx = Some(0);
        app.open_selected_group_chat();
        assert!(!app.unread_mentions.contains("rust"));
        assert!(!app.unread_messages.contains("rust"));
    }
}
