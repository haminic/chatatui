use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};

use crate::app::{ActivePanel, App, ConnectField, Screen};

pub fn render(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Connect => render_connect_screen(frame, app),
        Screen::Chat => render_chat_screen(frame, app),
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
                let is_active_chat = app.active_chat() == Some(user.as_str());
                let is_cursor = app.cursor_user_idx == Some(idx);

                let badge = if is_active_chat { "●" } else { " " };

                let content = if is_active_chat {
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

    // 2. Groups List (placeholder for DM phase per SPECS.md)
    let groups_p = Paragraph::new(vec![
        Line::from(Span::styled(
            " (Groups feature",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  coming soon)",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Groups (0) "),
    );
    frame.render_widget(groups_p, left_chunks[1]);

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
    let chat_title = match active_chat_opt {
        Some(user) => format!(" Chat with @{user} "),
        None => " Chat (No active conversation) ".to_string(),
    };

    let chat_block = Block::default()
        .borders(Borders::ALL)
        .title(chat_title)
        .padding(Padding::horizontal(1))
        .style(Style::default().fg(Color::White));

    let chat_inner_area = chat_block.inner(right_chunks[0]);
    frame.render_widget(chat_block, right_chunks[0]);

    if let Some(user) = active_chat_opt {
        let empty_vec = Vec::new();
        let history = app.chat_histories.get(user).unwrap_or(&empty_vec);

        if history.is_empty() {
            let empty_p = Paragraph::new(vec![Line::from(Span::styled(
                "No messages yet. Send a direct message below!",
                Style::default().fg(Color::DarkGray),
            ))])
            .alignment(Alignment::Center);
            frame.render_widget(empty_p, chat_inner_area);
        } else {
            let total_width = chat_inner_area.width as usize;
            let message_lines: Vec<Line> = history
                .iter()
                .map(|msg| {
                    let sender_str = if msg.is_me {
                        format!("<{}> ", app.my_username)
                    } else {
                        format!("<{}> ", msg.sender)
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
                    let sender_span = Span::styled(sender_str.clone(), sender_style);
                    let sender_width = sender_str.len();

                    let content_span = Span::raw(&msg.content);
                    let content_width = msg.content.chars().count();

                    let left_width = sender_width + content_width;

                    let right_item = if msg.is_pending {
                        Some((
                            Span::styled("⏳", Style::default().fg(Color::Yellow)),
                            2usize,
                        ))
                    } else if let Some(time_str) = msg.formatted_time() {
                        let time_width = time_str.len();
                        Some((
                            Span::styled(time_str, Style::default().fg(Color::DarkGray)),
                            time_width,
                        ))
                    } else {
                        None
                    };

                    if let Some((right_span, right_width)) = right_item {
                        if total_width > left_width + right_width {
                            let pad = total_width - left_width - right_width;
                            Line::from(vec![
                                sender_span,
                                content_span,
                                Span::raw(" ".repeat(pad)),
                                right_span,
                            ])
                        } else {
                            Line::from(vec![
                                sender_span,
                                content_span,
                                Span::raw(" "),
                                right_span,
                            ])
                        }
                    } else {
                        Line::from(vec![sender_span, content_span])
                    }
                })
                .collect();

            // Calculate auto-scroll if message lines exceed visible height
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
                "Browse users on the left with [j/k] and press [Enter] to open chat.",
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
    let input_title = match active_chat_opt {
        Some(user) => format!(" Message @{user} (Press Enter to send) "),
        None => " Message (Press Enter on a user to start chatting) ".to_string(),
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
            Span::styled("[Enter/l] ", Style::default().fg(Color::Cyan)),
            Span::raw("Open Chat   "),
            Span::styled("[Tab] ", Style::default().fg(Color::Cyan)),
            Span::raw("Focus Chat   "),
            Span::styled("[Ctrl+C] ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ],
        ActivePanel::Input => vec![
            Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
            Span::raw("Send Message   "),
            Span::styled("[Esc/Tab] ", Style::default().fg(Color::Cyan)),
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
