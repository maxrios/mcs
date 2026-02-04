#![warn(clippy::all, clippy::pedantic, clippy::nursery, unused_extern_crates)]

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::{SinkExt, StreamExt};
use protocol::{ChatPacket, McsCodec, Message};
use ratatui::{Terminal, backend::CrosstermBackend};
use rustls::{ClientConfig, RootCertStore, crypto::ring};
use rustls_pemfile::certs;
use rustls_pki_types::ServerName;
use std::{
    error::Error,
    fs::File,
    io::{self, BufReader},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::split,
    net::TcpStream,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    time::interval,
};
use tokio_rustls::{TlsConnector, client::TlsStream};
use tokio_util::codec::FramedRead;

mod app;
mod state;
use app::{AppState, ChatApp};
use state::{ChatClient, ChatEvent};

enum KeyEventResult {
    Connect,
    Continue,
    Break,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if ring::default_provider().install_default().is_err() {
        eprintln!("Failed to set default CryptoProvider");
    }

    let mut root_store = RootCertStore::empty();
    let file = File::open("tls/ca.cert").expect("Failed to open cert");
    let mut reader = BufReader::new(file);
    for cert in certs(&mut reader) {
        let _ = root_store.add(cert?);
    }
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));

    let (ui_tx, network_rx) = mpsc::unbounded_channel::<Message>();
    let (network_tx, mut ui_rx) = mpsc::unbounded_channel::<ChatEvent>();

    let mut pending_network_rx = Some(network_rx);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let term_backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(term_backend)?;

    let mut app = ChatApp::new(ui_tx);

    loop {
        terminal.draw(|f| app.update_ui(f))?;

        while let Ok(event) = ui_rx.try_recv() {
            if let ChatEvent::HistoryBatch(history) = event {
                let width = terminal.size()?.width.saturating_sub(2);
                app.handle_history_batch(history, width);
            } else {
                app.messages.push(event);
                app.scroll = app.scroll.saturating_add(1);
            }
        }

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match handle_key_event_with_optional_connect(key, &mut app) {
                KeyEventResult::Connect => {
                    init_tcp_connection(&mut app, &connector, &mut pending_network_rx, &network_tx)
                        .await;
                }
                KeyEventResult::Continue => {}
                KeyEventResult::Break => break,
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

async fn init_tcp_connection(
    app: &mut ChatApp,
    connector: &TlsConnector,
    pending_network_rx: &mut Option<UnboundedReceiver<Message>>,
    network_tx: &UnboundedSender<ChatEvent>,
) {
    let ip = app.login_ip.clone();
    let user = app.login_user.clone();
    let pass = app.login_pass.clone();
    let target = format!("{ip}:64400");

    match TcpStream::connect(&target).await {
        Ok(stream) => match ServerName::try_from(ip.clone()) {
            Ok(domain) => match connector.connect(domain, stream).await {
                Ok(tls_stream) => {
                    app.username = user;
                    app.state = AppState::Chat;
                    app.connection_error = None;

                    if let Some(rx) = pending_network_rx.take() {
                        spawn_event_listener(
                            tls_stream,
                            &app.username,
                            &pass,
                            network_tx.clone(),
                            rx,
                        )
                        .await;
                    }
                }
                Err(e) => {
                    app.connection_error = Some(format!("TLS Error: {e}"));
                }
            },
            Err(e) => {
                app.connection_error = Some(format!("Invalid IP for Cert: {e}"));
            }
        },
        Err(e) => {
            app.connection_error = Some(format!("Connection Failed: {e}"));
        }
    }
}

async fn spawn_event_listener(
    stream: TlsStream<TcpStream>,
    username: &str,
    password: &str,
    network_tx: UnboundedSender<ChatEvent>,
    mut network_rx: UnboundedReceiver<Message>,
) {
    let (reader, writer) = split(stream);

    let mut client = ChatClient::new(writer, username.into(), password.into());
    if let Err(e) = client.connect().await {
        let _ = network_tx.send(ChatEvent::Error(format!("Handshake error: {e}")));
        return;
    }

    let mut framed_reader = FramedRead::new(reader, McsCodec);
    tokio::spawn(async move {
        let mut heartbeat_timer = interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                result = framed_reader.next() => {
                    match result {
                        Some(Ok(Message::Chat(msg))) => {
                            if msg.sender == "server" {
                                let _ = network_tx.send(ChatEvent::SystemMessage(msg));
                            } else {
                                let _ = network_tx.send(ChatEvent::UserMessage(msg));
                            }
                        }
                        Some(Ok(Message::Error(err))) => {
                            let _ = network_tx.send(ChatEvent::Error(err.to_string()));
                        }
                        Some(Ok(Message::HistoryResponse(history))) => {
                            let _ = network_tx.send(ChatEvent::HistoryBatch(history));
                        }
                        None => {
                            let _ = network_tx.send(
                                ChatEvent::SystemMessage(
                                    ChatPacket::new_server_packet("Connection closed by server.".to_string())));
                            break;
                        }
                        _ => {}
                    }
                }
                Some(msg) = network_rx.recv() => {
                    if client.writer.send(msg).await.is_err() {
                        let _ = network_tx.send(ChatEvent::Error("Failed to send message".to_string()));
                    }
                }
                _ = heartbeat_timer.tick() => {
                    if client.writer.send(Message::Heartbeat).await.is_err() {
                            let _ = network_tx.send(ChatEvent::Error("Connection lost".to_string()));
                    }
                }
            }
        }
    });
}

fn handle_key_event_with_optional_connect(key: KeyEvent, app: &mut ChatApp) -> KeyEventResult {
    match app.state {
        AppState::Login => match key.code {
            KeyCode::Tab => {
                app.login_field_idx = (app.login_field_idx + 1) % 3;
            }
            KeyCode::Char(c) => match app.login_field_idx {
                0 => app.login_ip.push(c),
                1 => app.login_user.push(c),
                2 => app.login_pass.push(c),
                _ => {}
            },
            KeyCode::Backspace => match app.login_field_idx {
                0 => {
                    app.login_ip.pop();
                }
                1 => {
                    app.login_user.pop();
                }
                2 => {
                    app.login_pass.pop();
                }
                _ => {}
            },
            KeyCode::Enter => {
                if !app.login_ip.is_empty()
                    && !app.login_user.is_empty()
                    && !app.login_pass.is_empty()
                {
                    return KeyEventResult::Connect;
                }
                app.connection_error = Some("All fields are required".to_string());
            }
            KeyCode::Esc => return KeyEventResult::Break,
            _ => {}
        },
        AppState::Chat => match key.code {
            KeyCode::Enter => app.submit_message(),
            KeyCode::Char(c) => app.input.push(c),
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Up => {
                if app.scroll == 0 {
                    app.request_history();
                } else {
                    app.scroll = app.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if app.scroll < app.scroll_limit {
                    app.scroll = app.scroll.saturating_add(1);
                }
            }
            KeyCode::Esc => return KeyEventResult::Break,
            _ => {}
        },
    }

    KeyEventResult::Continue
}
