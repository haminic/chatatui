# Chatatui 💬

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Framework](https://img.shields.io/badge/TUI-Ratatui-green.svg)](https://ratatui.rs/)
[![Async](https://img.shields.io/badge/async-Tokio-blueviolet.svg)](https://tokio.rs/)

**Chatatui** is a fast, asynchronous terminal-based chat application built in Rust. It features a modern Terminal User Interface (TUI) powered by **Ratatui** and **Crossterm**, backed by a high-concurrency TCP socket server powered by **Tokio**.

Developed as a Term Project for **Computer Networks (Socket Programming)**, Chatatui demonstrates low-level socket communication, custom application-layer protocol design, decoupled asynchronous architecture, and rich terminal interfaces across different physical machines.

---

## 📋 Table of Contents
- [Project Specification & Score Checklist](#-project-specification--score-checklist)
  - [Part 1: System Architecture Design (1.5 pts)](#part-1-system-architecture-design-15-pts)
  - [Part 2: Fundamental Implementation (7.0 pts)](#part-2-fundamental-implementation-70-pts)
  - [Part 3: Special Points (4.0 pts)](#part-3-special-points-40-pts)
- [System Architecture & Network Protocols](#-system-architecture--network-protocols)
  - [High-Level Architecture](#high-level-architecture)
  - [Protocol Implementation](#protocol-implementation)
- [Comprehensive Features](#-comprehensive-features)
  - [Client TUI](#client-tui-ratatui)
  - [Server Engine](#server-engine-tokio)
- [Team Contributions](#-team-contributions)
- [Project Structure](#-project-structure)
- [Getting Started](#-getting-started)
  - [Prerequisites](#prerequisites)
  - [Running Locally](#running-locally)
  - [Multi-Computer / LAN Setup](#multi-computer--lan-setup-r1-compliance)
- [Keybindings & Controls Quick Reference](#-keybindings--controls-quick-reference)

---

## ✅ Project Specification & Score Checklist

This project is built strictly according to the course **Term Project: Socket Programming** specification (Full Score: 12.5 points).

### Part 1: System Architecture Design (1.5 pts)
| Req | Points | Description | Status | Implementation Details |
|:---:|:---:|:---|:---:|:---|
| **R1** | 1.0 | **Multi-Computer Architecture & Network Protocol Documentation**<br>Must run with at least 2 clients on different physical computers. Explain computer network protocols: what protocols are used, why selected, and how each is used. | Completed<br>`[x]` | Fully decoupled client and server binaries. Clients connect over TCP/IP across LAN/WAN. Complete network protocol rationale documented in [System Architecture & Network Protocols](#-system-architecture--network-protocols). |
| **R2** | 0.5 | **Pure Socket Programming**<br>Chat messages between server and each client must be implemented using socket programming only (no HTTP/REST frameworks). | Completed<br>`[x]` | Built using low-level asynchronous TCP stream sockets via `tokio::net::TcpListener` and `tokio::net::TcpStream` with line-delimited JSON framing. |

### Part 2: Fundamental Implementation (7.0 pts)
| Req | Points | Description | Status | Implementation Details |
|:---:|:---:|:---|:---:|:---|
| **R3** | 0.5 | **Unique Client Names**<br>Each client must have a unique name. Duplicate usernames are rejected. | Completed<br>`[x]` | Server verifies usernames upon registration, rejects existing names, and maintains a reserved pool (`used_usernames`) so disconnected names cannot be hijacked. |
| **R4** | 0.5 | **Connected Client List**<br>Each client can see a list of names of all clients currently connected to the server, including its own name. | Completed<br>`[x]` | Real-time `UserList` broadcast on join/leave. Client UI places own username at the top highlighted with `@username (you)`. |
| **R5** | 0.5 | **Dedicated Chat Rooms**<br>Each chat between clients (both Private and Group message) must have its own isolated chat room. | Completed<br>`[x]` | Separate conversation state and isolated chat history maps (`chat_histories`) for every 1-on-1 peer and group room. |
| **R6** | 0.5 | **Chat Box & Chat Window**<br>Each chat room must include a chat box for sending text messages and a chat window displaying sent and incoming messages. | Completed<br>`[x]` | Right panel features a dedicated message viewport with auto-scroll and formatted timestamps, plus a bottom text input box. |
| **R7** | 1.0 | **Private Direct Messaging**<br>Direct text message to any client in the list. Only sender and receiver can see the messages. | Completed<br>`[x]` | Direct message routing routed strictly by `ClientId` to target recipient with sender echo confirmation. Peers cannot intercept. Self-DM is safely disabled. |
| **R8** | 1.0 | **Group Creation**<br>Each client can create chat group(s), initially including only themselves as a member. | Completed<br>`[x]` | Modal group creation (`c` key). The server initializes the group with the creator as the sole initial member. |
| **R9** | 1.0 | **Group List & Member Inspection**<br>Each client can see a list of all existing chat groups created by any client, with member lists. | Completed<br>`[x]` | Real-time `GroupList` sync displays all groups categorized by Created/Joined/Public with live member counts. Pressing `m` opens the member list popup. |
| **R10** | 1.0 | **Voluntary Group Joining**<br>Clients choose to join a group chat only by themselves; they are not automatically added by the creator. | Completed<br>`[x]` | Clients select any public group and press `Enter` to join voluntarily. Group creator cannot force users into groups. |
| **R11** | 1.0 | **Group Broadcast Messaging**<br>Send text messages to joined groups. Only members of the chat group can see the messages. | Completed<br>`[x]` | Server verifies group membership prior to routing `GroupMessage`. Non-members cannot send or receive group messages. |

### Part 3: Special Points (4.0 pts)
> *Note: At least one feature must be an AI feature.*
- [ ] **AI Assistant / Chatbot Integration (AI Feature)** — Planned integration with an AI service for automated responses, conversation summaries, or sentiment analysis.
- [ ] **Password-Protected Private Groups** — Optional password authentication for private room entry.
- [ ] **Cloud-Based Deployment** — Deploying the server binary to a cloud provider (e.g., Fly.io, AWS EC2, or DigitalOcean) with public IP/domain.
- [ ] **@Mention Notifications** — Visual alert badge and sound/highlighting when a user's `@name` is mentioned in group chats.

---

## 🏗️ System Architecture & Network Protocols

### High-Level Architecture

```
                      +-----------------------------+
                      |       Chatatui Server       |
                      |   (tokio::net::TcpListener) |
                      |    In-Memory ServerState    |
                      +--------------+--------------+
                                     |
               +---------------------+---------------------+
               | (TCP Socket / LAN)                        | (TCP Socket / LAN)
               v                                           v
+-----------------------------+             +-----------------------------+
|    Physical Machine A       |             |     Physical Machine B      |
|   Client 1: @alice          |             |    Client 2: @bob           |
|  - Ratatui TUI Main Loop    |             |  - Ratatui TUI Main Loop    |
|  - Tokio Network Worker     |             |  - Tokio Network Worker     |
|  - mpsc In/Out Channels     |             |  - mpsc In/Out Channels     |
+-----------------------------+             +-----------------------------+
```

### Protocol Implementation

#### 1. Transport Layer Protocol: TCP (Transmission Control Protocol)
* **What is used:** `tokio::net::TcpListener` on the server and `tokio::net::TcpStream` on the client.
* **Why TCP was selected:**
  * **Reliability:** Chat messages require 100% data integrity. TCP provides guaranteed delivery via acknowledgments (ACK) and automatic retransmissions, ensuring no messages are dropped or corrupted.
  * **In-Order Delivery:** Packet sequencing guarantees that conversation messages and announcements arrive in the exact order they were sent.
  * **Connection State Management:** Connection-oriented sockets make it straightforward to detect client disconnects via socket EOF (`Ok(None)`), triggering prompt server cleanup and membership broadcasts.
* **How it is used:** The server listens on port `8054` (or user-specified port). Clients establish full-duplex TCP connections. Dedicated asynchronous tasks handle socket read and write streams independently using `tokio::io::split`.

#### 2. Application & Presentation Layer: Line-Delimited JSON Framing
* **What is used:** JSON-encoded payloads delimited by newline characters (`\n`), serialized and deserialized using `serde` and `serde_json`.
* **Why this was selected:**
  * **TCP Stream Framing:** TCP is a continuous byte stream without built-in message boundaries. Without application-level framing, messages can stick together (packet coadunation) or fragment across packets. Newline delimitation (`\n`) provides deterministic, lightweight message boundaries.
  * **Extensibility & Readability:** JSON is human-readable, self-describing, and strongly typed through Rust `enum` variants with `#[serde(tag = "type")]`.
* **Message Protocol Examples:**
  * Client registration: `{"type":"register","username":"alice"}\n`
  * Direct message: `{"type":"direct_message","recipient":"bob","content":"hello"}\n`
  * Group message: `{"type":"group_message","group":"rustacean","content":"hi all"}\n`
  * Server broadcast: `{"type":"group_list","groups":[{"name":"rustacean","creator":"alice","members":["alice","bob"]}]}\n`

#### 3. Concurrency & Event Loop Model
* Non-blocking architecture using **Tokio**:
  * **Network Worker Task:** Spawns a background task running split reader and writer loops.
  * **TUI Render Loop:** The main thread runs at 30ms intervals with Ratatui and Crossterm event streams, completely non-blocking.
  * **Inter-Task Communication:** Outbound messages and inbound server events communicate via lock-free Tokio unbounded channels (`mpsc::unbounded_channel`).

---

## 🌟 Comprehensive Features

### Client TUI (Ratatui)
* **Screen 1: Connect Screen**
  * Interactive input form for Server IP/Port and desired username.
  * Real-time client-side name validation (`[a-z0-9_]`, minimum 3 characters).
  * Direct error feedback on connection failures or taken usernames.
  * Tab, Up, and Down arrow keys to switch fields smoothly.
* **Screen 2: Multi-Panel Chat Dashboard**
  * **Top Header:** Displays application branding and current user badge (`@username`).
  * **Left Panel (Contacts & Groups):**
    * **Connected Users:** Displays all online users. Your own name is pinned at the top with `(you)` styling. Active conversation indicator (`●`). Self-DM is safely disabled.
    * **Hierarchical Group Directory:** Groups are organized into:
      1. Groups you created (`[created]`)
      2. Groups you joined (`[joined]`)
      3. Other available groups (`[public]` or `[Enter to join]`)
    * Dynamic group member counts displayed in real time: `#group [role] (count)`.
  * **Right Panel (Conversation & Messaging):**
    * **Chat History Window:** Isolated message history per room (DM or Group).
    * **Formatted Timestamps:** Messages display 12-hour local timestamps (`02:30 PM`) placed neatly before the username.
    * **Message Status Badges:** Pending messages display `⏳` until server delivery acknowledgment.
    * **System Announcements:** Join/leave announcements (`* @alice joined the group *`) highlighted in italic center text.
    * **Auto-Scrolling:** Automatically scrolls to the latest incoming messages.
    * **Message Input Box:** Live editing with prompt, cursor indicator, and Enter to send.
* **Interactive Dialogs & Modals:**
  * **Create Group Modal (`c`):** Prompt to enter group name with instant validation.
  * **Leave Group Modal (`l`):** Confirmation dialog (`[y/n]`). Automatically cleans up room state, wipes chat history, and resets view to no active conversation.
  * **Delete Group Modal (`d`):** Protected action only accessible by the group creator.
  * **View Members Modal (`m` or `v`):** Scrollable popup displaying all members currently in the highlighted group.
* **Context-Aware Dynamic Action Bar:**
  * The bottom footer dynamically displays only valid hotkeys based on whether the user is in the Users list, Groups list (with different keys for creator vs member vs visitor), or Chat input.
  * Non-destructive navigation: Pressing `Esc` in the input box returns to the side panel rather than closing the program.

### Server Engine (Tokio)
* **High-Concurrency Socket Handling:** Decoupled in-memory server state protected by `Arc<Mutex<ServerState>>`.
* **Strict Unique Identity Enforcement:** Rejects duplicate usernames and keeps track of disconnected usernames to prevent impersonation during session lifetime.
* **Direct Message Routing:** Routes messages strictly between sender and receiver. Non-participants never receive direct message frames.
* **Group Chat Management:**
  * Group creation with creator as sole initial member.
  * Voluntary join and leave handling with system broadcast notices.
  * Creator-only deletion authority.
  * Automatic group deletion if creator disconnects from the server.
  * Automatic removal of disconnected users from all joined groups.
* **Real-Time State Synchronization:** Broadcasts updated `UserList` and `GroupList` whenever any client connects, disconnects, creates, joins, leaves, or deletes a group.

---

## 👥 Team Contributions

All team members contributed across the architecture, networking, protocol design, and client UI:

* **`haminic`**:
  * Initial Rust workspace architecture (`common`, `server`, `client`).
  * Asynchronous TCP socket networking and framing with Tokio.
  * Server direct message routing and message acknowledgment echo logic.
  * Core TUI client scaffolding with Ratatui event loop and non-blocking channels.
  * Error handling and packet serialization/deserialization.
* **`duckin`**:
  * Group chat backend and client protocol integration.
  * Group creation, joining, leaving, and deletion mechanics.
  * Broadcast distribution strictly filtered to active group members.
  * In-memory group state tracking (`GroupInfo`, creators, and member sets).
* **`NoHaxJustTuL`**:
  * Unique username enforcement and disconnected username reservation (R3).
  * Contact list formatting: own username listed first with `(you)` badge; self-DM prevention.
  * Group directory ranking, dynamic badges (`[created]`, `[joined]`, `[public]`), and auto-open chat on join.
  * Formatted local timestamps (`HH:MM AM/PM`) displayed before sender usernames.
  * Group exit lifecycle: chat history wiping and reset to inactive conversation state.
  * Interactive modal dialogs: Group creation, Leave confirmation, Delete confirmation, and Member inspection.
  * Dynamic context-aware bottom action bar and keyboard navigation safety (`Esc` handling, Vim keys).

---

## 📁 Project Structure

```
chatatui/
├── Cargo.toml               # Workspace configuration
├── README.md                # Project documentation & requirements checklist
├── SPECS.md                 # Design specification document
├── common/                  # Shared library crate
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs           # ClientMessage, ServerMessage, GroupInfo, validation
├── server/                  # TCP Socket Server binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs          # Server entrypoint and CLI address binding
│       └── lib.rs           # In-memory ServerState, message dispatcher, connection handler
└── client/                  # Terminal User Interface client binary
    ├── Cargo.toml
    └── src/
        ├── main.rs          # Crossterm terminal setup and event loop
        ├── lib.rs           # Module declarations
        ├── app.rs           # Client application state, history, actions, unit tests
        ├── ui.rs            # Ratatui UI rendering (screens, panels, modals)
        └── network.rs       # Background Tokio TCP worker and channel bridging
```

---

## 🚀 Getting Started

### Prerequisites
* [Rust toolchain](https://www.rust-lang.org/tools/install) (Edition 2024 / Rust 1.85+ recommended).

### Running Locally

1. **Clone the repository:**
   ```bash
   git clone <repo-url>
   cd chatatui
   ```

2. **Start the server:**
   ```bash
   # Starts server on default 127.0.0.1:8054
   cargo run -p server

   # Or specify a custom address/port
   cargo run -p server -- 127.0.0.1:8080
   ```

3. **Start one or more clients:**
   ```bash
   cargo run -p client
   ```
   * Enter the server address (e.g. `127.0.0.1:8054`) and choose a username (e.g., `alice`).
   * In another terminal window, start a second client with a different username (e.g., `bob`).

4. **Run automated test suite:**
   ```bash
   cargo test --workspace
   ```

---

### 🌐 Multi-Computer / LAN Setup (R1 Compliance)

To satisfy requirement **R1** (running at least two clients on different physical computers):

1. **Determine the server host's Local IP Address:**
   * **macOS / Linux:** `ifconfig | grep "inet " | grep -v 127.0.0.1` or `ip a`
   * **Windows:** `ipconfig` (Look for IPv4 Address, e.g., `192.168.1.50`).

2. **Start the server bound to all network interfaces:**
   ```bash
   # On Computer 1 (Server Host):
   cargo run -p server -- 0.0.0.0:8054
   ```

3. **Connect Client 1 (Computer 1 or Computer 2):**
   ```bash
   cargo run -p client
   ```
   * Set Server IP: `<SERVER_LOCAL_IP>:8054` (e.g. `192.168.1.50:8054`).
   * Set Username: `client_one`.

4. **Connect Client 2 (Different Physical Computer):**
   ```bash
   cargo run -p client
   ```
   * Set Server IP: `<SERVER_LOCAL_IP>:8054` (e.g. `192.168.1.50:8054`).
   * Set Username: `client_two`.

*Both clients can now exchange private DMs, create groups, inspect member lists, and participate in multi-user group chats across physical machines!*

---

## ⌨️ Keybindings & Controls Quick Reference

| Context | Key | Action |
|:---|:---:|:---|
| **Connect Screen** | `Tab` / `↑` / `↓` | Switch between Server Address and Username fields |
| | `Enter` | Attempt connection to server |
| | `Ctrl + C` | Quit application |
| **Users Panel** | `j` / `↓` | Navigate to next user |
| | `k` / `↑` | Navigate to previous user |
| | `Enter` / `l` / `→` | Open direct message chat with selected user |
| | `Tab` | Switch focus to Groups Panel |
| | `c` | Open Create Group dialog |
| | `q` / `Ctrl + C` | Quit application |
| **Groups Panel** | `j` / `↓` | Navigate to next group |
| | `k` / `↑` | Navigate to previous group |
| | `Enter` | Open chat (if joined) / Join group and open chat |
| | `m` / `v` | View member list of highlighted group |
| | `l` | Leave group (members only; opens confirmation) |
| | `d` | Delete group (creator only; opens confirmation) |
| | `c` | Open Create Group dialog |
| | `Tab` | Focus Chat Input (if chat is open) or Users Panel |
| | `h` / `←` / `BackTab` | Return focus to Users Panel |
| **Chat Input** | `Typing` | Type chat message |
| | `Enter` | Send message to active conversation |
| | `Backspace` | Delete last character |
| | `Esc` | Unfocus input and return to Users / Groups Panel |
| | `Tab` | Switch focus to Users Panel |
| **Modal Dialogs** | `y` / `Enter` | Confirm action (Create / Leave / Delete) |
| | `n` / `Esc` | Cancel modal and return to chat |
