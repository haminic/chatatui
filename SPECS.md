Create a Rust workspace containing two separate binaries: `server` and `client` (TUI), communicating over asynchronous sockets. This is for a Computer Networks project--keep it simple, do not overengineer. Keep tests simple, do not make it too detailed.

### General Requirements

1. **Architecture & Scope:**
   - Pure in-memory state (no persistent databases). We do not store chat histories: messages are sent to the receiver and that's it.
   - Cleanly decoupled server and client binaries runnable on separate machines.

2. **Server Responsibilities:**
   - Default server port is `8054`.
   - Track connected clients with unique usernames; reject duplicates.
   - Manage chat groups: track group names, creators, and active member sets.
   - Manage group actions: for a group the user created, they may delete; for a group the user joined, they may leave; and for other groups, they may join.
   - Handle routing for private direct messages (1-on-1) and group broadcasts strictly to joined members.
   - Broadcast updated user lists and group lists whenever clients join or leave.
   - Graceful disconnect cleanup: when a client disconnects, remove them from all joined groups, delete any groups they created, and notify remaining clients of any removal.
   - Usernames and group names should be at least 3 characters of `[a-z0-9_]`

3. **Client TUI (ratatui):**
   - It must manage the chat history. If a group is deleted or a user disconnects, just wipe the history.
   - **Screen 1 (Connect):** Simple input form for server IP/port and desired username. Show error feedback if the name is taken.
   - **Screen 2 (Chat Interface):**
     - **Left Panel:** Consists of 2 parts: List of all connected users and list of all active groups (selecting an item in this panel will open the chat in the right panel). For the groups section, each group may be joined freely by anyone. The order of the groups are: groups you created, groups you joined, followed by other groups. There should also be a button to create a new group at the bottom, which will show a pop-up to enter the group name. Show error if group name taken.
     - **Right Panel:** Active conversation view displaying isolated message history for the focused room (DM or Group) and an input box at the bottom to send messages.
     - **Actions:** Action keybinds are displayed at the bottom. Some important actions include:
      - leaving a group if a group the user joined is selected (shows "are you sure?" pop-up)
      - deleting a group if a group the user created is selected (shows "are you sure?" pop-up)
      - joining a group if the user isn't in that group
      - focus chat if a DM or a group the user's joined is selected
      - view members if a group is selected (shows a pop-up)
      - other general actions: h, j, k, l for movement, etc.
   - Asynchronous network worker loop that updates TUI state without blocking UI rendering or input handling.
