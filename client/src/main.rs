use std::io::{self, stdout};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use client::app::{ActivePanel, App, ConnectField, Screen};
use client::network::{spawn_network_worker, NetworkEvent};
use client::ui;
use common::ClientMessage;

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal).await;

    restore_terminal()?;

    if let Err(err) = res {
        eprintln!("Application error: {err}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    let mut reader = crossterm::event::EventStream::new();
    let mut interval = tokio::time::interval(Duration::from_millis(30));

    let (mut outbound_tx, _) = mpsc::unbounded_channel::<ClientMessage>();
    let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel::<NetworkEvent>();
    let mut worker_handle: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                terminal.draw(|f| ui::render(f, &app))?;
            }

            Some(net_ev) = inbound_rx.recv() => {
                match net_ev {
                    NetworkEvent::Connected => {
                        let _ = outbound_tx.send(ClientMessage::Register {
                            username: app.username_input.clone(),
                        });
                    }
                    NetworkEvent::Server(msg) => {
                        app.handle_server_message(msg);
                    }
                    NetworkEvent::Error(err) => {
                        if app.screen == Screen::Connect {
                            app.connect_error = Some(err);
                            app.is_connecting = false;
                        } else {
                            app.status_message = Some(err);
                        }
                    }
                    NetworkEvent::Disconnected(reason) => {
                        app.on_network_disconnected(&reason);
                    }
                }
            }

            Some(Ok(event)) = reader.next() => {
                if let Event::Key(key) = event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    // Global quit on Ctrl+C
                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                        break;
                    }

                    match app.screen {
                        Screen::Connect => {
                            match key.code {
                                KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
                                    app.connect_field = match app.connect_field {
                                        ConnectField::ServerAddress => ConnectField::Username,
                                        ConnectField::Username => ConnectField::ServerAddress,
                                    };
                                }
                                KeyCode::Enter => {
                                    if let Err(err) = app.validate_connect_input() {
                                        app.connect_error = Some(err);
                                    } else {
                                        app.connect_error = None;
                                        app.is_connecting = true;

                                        // Abort previous worker if any
                                        if let Some(h) = worker_handle.take() {
                                            h.abort();
                                        }

                                        let (new_out_tx, new_out_rx) = mpsc::unbounded_channel();
                                        outbound_tx = new_out_tx;

                                        worker_handle = Some(spawn_network_worker(
                                            app.server_address.clone(),
                                            new_out_rx,
                                            inbound_tx.clone(),
                                        ));
                                    }
                                }
                                KeyCode::Backspace => {
                                    match app.connect_field {
                                        ConnectField::ServerAddress => {
                                            app.server_address.pop();
                                        }
                                        ConnectField::Username => {
                                            app.username_input.pop();
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    match app.connect_field {
                                        ConnectField::ServerAddress => {
                                            app.server_address.push(c);
                                        }
                                        ConnectField::Username => {
                                            app.username_input.push(c);
                                        }
                                    }
                                }
                                KeyCode::Esc => {
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Screen::Chat => {
                            match app.active_panel {
                                ActivePanel::Users => {
                                    match key.code {
                                        KeyCode::Char('j') | KeyCode::Down => {
                                            app.select_next_user();
                                        }
                                        KeyCode::Char('k') | KeyCode::Up => {
                                            app.select_prev_user();
                                        }
                                        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                                            app.open_selected_chat();
                                        }
                                        KeyCode::Tab => {
                                            app.focus_chat();
                                        }
                                        KeyCode::Char('q') | KeyCode::Esc => {
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                                ActivePanel::Input => {
                                    match key.code {
                                        KeyCode::Enter => {
                                            if let Some(msg) = app.send_current_message() {
                                                let _ = outbound_tx.send(msg);
                                            }
                                        }
                                        KeyCode::Backspace => {
                                            app.input_message.pop();
                                        }
                                        KeyCode::Esc | KeyCode::Tab => {
                                            app.active_panel = ActivePanel::Users;
                                        }
                                        KeyCode::Left if app.input_message.is_empty() => {
                                            app.active_panel = ActivePanel::Users;
                                        }
                                        KeyCode::Char(c) => {
                                            app.input_message.push(c);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(h) = worker_handle {
        h.abort();
    }

    Ok(())
}
