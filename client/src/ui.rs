use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};

use crate::app::{ActivePanel, App, ConnectField, Modal, Screen};

pub fn render(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Connect => render_connect_screen(frame, app),
        Screen::Chat => {
            render_chat_screen(frame, app);
            if let Some(ref modal) = app.modal {
                render_modal(frame, modal);
            }
        }
    }
}

/// Helper function to create a centered Rect for dialogs/popups.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_connect_screen(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Background container
    let bg_block = Block::default()
        .borders(Borders::ALL)
        .title(" Chatatui ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(bg_block, size);

    // Centered modal popup
    let area = centered_rect(60, 50, size);
    frame.render_widget(Clear, area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .title(" Connect to Server ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::White));
    frame.render_widget(modal_block.clone(), area);

    let inner = modal_block.inner(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Server Address
            Constraint::Length(3), // Username
            Constraint::Length(2), // Status / Error feedback
            Constraint::Min(1),    // Filler / Instructions
        ])
        .split(inner);

    // Server Address Input
    let addr_style = if app.connect_field == ConnectField::ServerAddress {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let addr_cursor = if app.connect_field == ConnectField::ServerAddress {
        "_"
    } else {
        ""
    };
    let addr_p = Paragraph::new(format!("{}{}", app.server_address, addr_cursor)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(addr_style)
            .title(" Server IP:Port (e.g. 127.0.0.1:8054) "),
    );
    frame.render_widget(addr_p, chunks[0]);

    // Username Input
    let user_style = if app.connect_field == ConnectField::Username {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let user_cursor = if app.connect_field == ConnectField::Username {
        "_"
    } else {
        ""
    };
    let user_p = Paragraph::new(format!("{}{}", app.username_input, user_cursor)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(user_style)
            .title(" Desired Username [a-z0-9_] "),
    );
    frame.render_widget(user_p, chunks[1]);

    // Feedback message (Connecting status or Error)
    if app.is_connecting {
        let msg = Paragraph::new(Line::from(vec![Span::styled(
            "Connecting to server...",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));
        frame.render_widget(msg, chunks[2]);
    } else if let Some(ref err) = app.connect_error {
        let msg = Paragraph::new(Line::from(vec![Span::styled(
            format!("Error: {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]));
        frame.render_widget(msg, chunks[2]);
    }

    // Keybind hints
    let hints = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("[Tab/Up/Down] ", Style::default().fg(Color::Cyan)),
            Span::raw("Switch Field   "),
            Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
            Span::raw("Connect   "),
            Span::styled("[Esc/Ctrl+C] ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ]),
        Line::from(vec![Span::styled(
            "Username must be >= 3 characters of [a-z0-9_]",
            Style::default().fg(Color::DarkGray),
        )]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(hints, chunks[3]);
}

fn render_chat_screen(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Main layout: Header, Body, Bottom Bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top Header
            Constraint::Min(5),    // Body (Left & Right panels)
            Constraint::Length(1), // Bottom Action Bar
        ])
        .split(size);

    // Top Header
    let header_line = Line::from(vec![
        Span::styled(
            " Chatatui ",
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Logged in as: "),
        Span::styled(
            format!("@{}", app.my_username),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(header_line), main_chunks[0]);

    // Body: Split horizontally into Left Panel and Right Panel
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28), // Left Panel (Users / Groups)
            Constraint::Min(20),    // Right Panel (Chat view & input)
        ])
        .split(main_chunks[1]);

    // Left Panel: Split vertically into Users (top) and Groups (bottom)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(65), // Users list
            Constraint::Percentage(35), // Groups list (placeholder for DM phase)
        ])
        .split(body_chunks[0]);

    // 1. Users List
    let users_border_style = if app.active_panel == ActivePanel::Users {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let users_items: Vec<ListItem> = if app.users.is_empty() {
        vec![ListItem::new(Span::styled(
            " (no users online)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.users
            .iter()
            .enumerate()
            .map(|(idx, user)| {
                let is_me = user == &app.my_username;
                let is_active_chat = app.active_chat() == Some(user.as_str());
                let is_cursor = app.cursor_user_idx == Some(idx);

                let badge = if is_active_chat { "●" } else { " " };

                let content = if is_me {
                    format!("{badge} @{user} (you)")
                } else if is_active_chat {
                    format!("{badge} @{user} [active]")
                } else {
                    format!("{badge} @{user}")
                };

                let style = if is_cursor && app.active_panel == ActivePanel::Users {
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if is_cursor {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else if is_active_chat {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if is_me {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(Span::styled(content, style))
            })
            .collect()
    };

    let users_list = List::new(users_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(users_border_style)
            .title(format!(" Connected Users ({}) ", app.users.len())),
    );
    let mut state = ListState::default();
    state.select(app.cursor_user_idx);
    frame.render_stateful_widget(users_list, left_chunks[0], &mut state);

    // 2. Groups List (R9 & SPECS.md)
    let groups_border_style = if app.active_panel == ActivePanel::Groups {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let ordered_grps = app.ordered_groups();
    let groups_items: Vec<ListItem> = if ordered_grps.is_empty() {
        vec![ListItem::new(Span::styled(
            " (no groups created yet)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        ordered_grps
            .iter()
            .enumerate()
            .map(|(idx, grp)| {
                let is_active_chat = app.active_chat_group.as_deref() == Some(&grp.name);
                let is_cursor = app.cursor_group_idx == Some(idx);
                let is_creator = grp.creator == app.my_username;
                let is_member = grp.members.contains(&app.my_username);

                let badge = if is_active_chat { "●" } else { " " };
                let role_badge = if is_creator {
                    "[created]"
                } else if is_member {
                    "[joined]"
                } else {
                    "[join]"
                };

                let content = format!(
                    "{badge} #{name} {role} ({count})",
                    name = grp.name,
                    role = role_badge,
                    count = grp.members.len(),
                );

                let style = if is_cursor && app.active_panel == ActivePanel::Groups {
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if is_cursor {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else if is_active_chat {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if is_member {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };

                ListItem::new(Span::styled(content, style))
            })
            .collect()
    };

    let groups_list = List::new(groups_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(groups_border_style)
            .title(format!(" Groups ({}) [c: New] ", app.groups.len())),
    );
    let mut grp_state = ListState::default();
    grp_state.select(app.cursor_group_idx);
    frame.render_stateful_widget(groups_list, left_chunks[1], &mut grp_state);

    // Right Panel: Split vertically into Chat History (top) and Message Input (bottom)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Chat Messages
            Constraint::Length(3), // Input Box
        ])
        .split(body_chunks[1]);

    // Chat History View
    let active_chat_opt = app.active_chat();
    let chat_title = if let Some(ref grp_name) = app.active_chat_group {
        format!(" Group Chat #{grp_name} ")
    } else if let Some(ref user) = app.active_chat_user {
        format!(" Chat with @{user} ")
    } else {
        " Chat (No active conversation) ".to_string()
    };

    let chat_block = Block::default()
        .borders(Borders::ALL)
        .title(chat_title)
        .padding(Padding::horizontal(1))
        .style(Style::default().fg(Color::White));

    let chat_inner_area = chat_block.inner(right_chunks[0]);
    frame.render_widget(chat_block, right_chunks[0]);

    if active_chat_opt.is_some() {
        let history = app.current_chat_history();

        if history.is_empty() {
            let empty_p = Paragraph::new(vec![Line::from(Span::styled(
                "No messages yet. Send a message below!",
                Style::default().fg(Color::DarkGray),
            ))])
            .alignment(Alignment::Center);
            frame.render_widget(empty_p, chat_inner_area);
        } else {
            let message_lines: Vec<Line> = history
                .iter()
                .map(|msg| {
                    if msg.sender == "system" {
                        Line::from(Span::styled(
                            format!("* {} *", msg.content),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::ITALIC),
                        ))
                        .alignment(Alignment::Center)
                    } else {
                        let sender_name = if msg.is_me {
                            &app.my_username
                        } else {
                            &msg.sender
                        };

                        let time_str = if msg.is_pending {
                            "⏳ ".to_string()
                        } else if let Some(t) = msg.formatted_time() {
                            format!("{t} ")
                        } else {
                            "".to_string()
                        };

                        let time_style = if msg.is_pending {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        };

                        let sender_style = if msg.is_me {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        };

                        let time_span = Span::styled(time_str, time_style);
                        let sender_span = Span::styled(format!("{sender_name}: "), sender_style);
                        let content_span = Span::raw(&msg.content);

                        Line::from(vec![time_span, sender_span, content_span])
                            .alignment(Alignment::Left)
                    }
                })
                .collect();

            // Auto-scroll to bottom of chat
            let visible_height = chat_inner_area.height as usize;
            let total_lines = message_lines.len();
            let scroll_offset = if total_lines > visible_height {
                (total_lines - visible_height) as u16
            } else {
                0
            };

            let messages_p = Paragraph::new(message_lines)
                .wrap(Wrap { trim: false })
                .scroll((scroll_offset, 0));
            frame.render_widget(messages_p, chat_inner_area);
        }
    } else {
        let no_sel_p = Paragraph::new(vec![
            Line::from(Span::styled(
                "No active conversation.",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::styled(
                "Browse users/groups on the left and press [Enter] to open chat.",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(no_sel_p, chat_inner_area);
    }

    // Message Input Box
    let input_border_style = if app.active_panel == ActivePanel::Input {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let input_cursor = if app.active_panel == ActivePanel::Input {
        "_"
    } else {
        ""
    };
    let input_title = if let Some(ref grp_name) = app.active_chat_group {
        format!(" Message #{grp_name} (Press Enter to send) ")
    } else if let Some(ref user) = app.active_chat_user {
        format!(" Message @{user} (Press Enter to send) ")
    } else {
        " Message (Select a user or joined group to start chatting) ".to_string()
    };

    let input_p = Paragraph::new(format!("{}{}", app.input_message, input_cursor)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(input_border_style)
            .title(input_title),
    );
    frame.render_widget(input_p, right_chunks[1]);

    // Bottom Action Bar
    let action_spans = match app.active_panel {
        ActivePanel::Users => vec![
            Span::styled("[j/k] ", Style::default().fg(Color::Cyan)),
            Span::raw("Browse Users   "),
            Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
            Span::raw("Open DM   "),
            Span::styled("[Tab] ", Style::default().fg(Color::Cyan)),
            Span::raw("Groups Panel   "),
            Span::styled("[c] ", Style::default().fg(Color::Cyan)),
            Span::raw("Create Group   "),
            Span::styled("[Ctrl+C] ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ],
        ActivePanel::Groups => vec![
            Span::styled("[j/k] ", Style::default().fg(Color::Cyan)),
            Span::raw("Browse Groups   "),
            Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
            Span::raw("Open Chat   "),
            Span::styled("[j] ", Style::default().fg(Color::Cyan)),
            Span::raw("Join   "),
            Span::styled("[l] ", Style::default().fg(Color::Cyan)),
            Span::raw("Leave   "),
            Span::styled("[d] ", Style::default().fg(Color::Cyan)),
            Span::raw("Delete   "),
            Span::styled("[m] ", Style::default().fg(Color::Cyan)),
            Span::raw("Members   "),
            Span::styled("[c] ", Style::default().fg(Color::Cyan)),
            Span::raw("New Group   "),
            Span::styled("[Tab] ", Style::default().fg(Color::Cyan)),
            Span::raw("Focus Chat"),
        ],
        ActivePanel::Input => vec![
            Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
            Span::raw("Send Message   "),
            Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
            Span::raw("Exit Chat   "),
            Span::styled("[Tab] ", Style::default().fg(Color::Cyan)),
            Span::raw("Browse Users   "),
            Span::styled("[Ctrl+C] ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ],
    };

    let status_span = if let Some(ref status) = app.status_message {
        vec![
            Span::raw(" | "),
            Span::styled(status, Style::default().fg(Color::Magenta)),
        ]
    } else {
        vec![]
    };

    let mut full_spans = action_spans;
    full_spans.extend(status_span);

    frame.render_widget(Paragraph::new(Line::from(full_spans)), main_chunks[2]);
}

fn render_modal(frame: &mut Frame, modal: &Modal) {
    let size = frame.area();

    match modal {
        Modal::CreateGroup { input, error } => {
            let area = centered_rect(50, 30, size);
            frame.render_widget(Clear, area);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Create New Group ")
                .title_alignment(Alignment::Center)
                .style(Style::default().fg(Color::White));
            frame.render_widget(block.clone(), area);

            let inner = block.inner(area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3), // input
                    Constraint::Length(2), // error
                    Constraint::Length(1), // hints
                ])
                .split(inner);

            let input_p = Paragraph::new(format!("{input}_")).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    .title(" Group Name [a-z0-9_] "),
            );
            frame.render_widget(input_p, chunks[0]);

            if let Some(err) = error {
                let err_p = Paragraph::new(Span::styled(
                    format!("Error: {err}"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
                frame.render_widget(err_p, chunks[1]);
            }

            let hints = Paragraph::new(Line::from(vec![
                Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
                Span::raw("Create   "),
                Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
                Span::raw("Cancel"),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(hints, chunks[2]);
        }
        Modal::ConfirmLeaveGroup { group_name } => {
            let area = centered_rect(45, 20, size);
            frame.render_widget(Clear, area);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Leave Group ")
                .title_alignment(Alignment::Center)
                .style(Style::default().fg(Color::White));
            frame.render_widget(block.clone(), area);

            let inner = block.inner(area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(2), Constraint::Length(1)])
                .split(inner);

            let text = Paragraph::new(format!("Are you sure you want to leave #{group_name}?"))
                .alignment(Alignment::Center);
            frame.render_widget(text, chunks[0]);

            let hints = Paragraph::new(Line::from(vec![
                Span::styled("[y/Enter] ", Style::default().fg(Color::Yellow)),
                Span::raw("Leave   "),
                Span::styled("[n/Esc] ", Style::default().fg(Color::Cyan)),
                Span::raw("Cancel"),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(hints, chunks[1]);
        }
        Modal::ConfirmDeleteGroup { group_name } => {
            let area = centered_rect(50, 22, size);
            frame.render_widget(Clear, area);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .title(" Delete Group ")
                .title_alignment(Alignment::Center);
            frame.render_widget(block.clone(), area);

            let inner = block.inner(area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(2), Constraint::Length(1)])
                .split(inner);

            let text = Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("Delete group #{group_name}?"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw("This will delete the group for all members.")),
            ])
            .alignment(Alignment::Center);
            frame.render_widget(text, chunks[0]);

            let hints = Paragraph::new(Line::from(vec![
                Span::styled("[y/Enter] ", Style::default().fg(Color::Red)),
                Span::raw("Delete   "),
                Span::styled("[n/Esc] ", Style::default().fg(Color::Cyan)),
                Span::raw("Cancel"),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(hints, chunks[1]);
        }
        Modal::ViewMembers {
            group_name,
            members,
        } => {
            let area = centered_rect(45, 45, size);
            frame.render_widget(Clear, area);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" Members of #{group_name} ({}) ", members.len()))
                .title_alignment(Alignment::Center);
            frame.render_widget(block.clone(), area);

            let inner = block.inner(area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(inner);

            let items: Vec<ListItem> = members
                .iter()
                .map(|m| {
                    ListItem::new(Span::styled(
                        format!("  @{m}"),
                        Style::default().fg(Color::Cyan),
                    ))
                })
                .collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Members "));
            frame.render_widget(list, chunks[0]);

            let hints = Paragraph::new(Line::from(vec![
                Span::styled("[Esc/Enter] ", Style::default().fg(Color::Cyan)),
                Span::raw("Close"),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(hints, chunks[1]);
        }
    }
}
