use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
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
use tokio::{io::split, net::TcpStream, sync::mpsc, time::interval};
use tokio_rustls::TlsConnector;
use tokio_util::codec::FramedRead;

mod app;
mod state;
use app::ChatApp;
use state::ChatClient;

enum ChatEvent {
    MessageReceived(ChatPacket),
    SystemMessage(ChatPacket),
    Error(String),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if ring::default_provider().install_default().is_err() {
        panic!("Failed to set default CryptoProvider");
    }

    let args: Vec<String> = std::env::args().collect();
    let username = match args.get(1) {
        Some(u) => u.clone(),
        None => {
            eprintln!("Usage: chat <username>");
            return Ok(());
        }
    };

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

    let stream = match TcpStream::connect("0.0.0.0:64400").await {
        Ok(res) => res,
        Err(_) => {
            eprintln!("Error: Could not connect to server at 0.0.0.0:64400");
            return Ok(());
        }
    };

    let domain = ServerName::try_from("localhost")?;
    let stream = connector.connect(domain, stream).await?;

    let (reader, writer) = split(stream);
    let mut framed_reader = FramedRead::new(reader, McsCodec);
    let (ui_tx, mut network_rx) = mpsc::unbounded_channel::<ChatPacket>();
    let (network_tx, mut ui_rx) = mpsc::unbounded_channel::<ChatEvent>();

    let mut client = ChatClient::new(writer, username.clone());
    client.connect(&mut framed_reader).await;

    let net_notifier = network_tx.clone();
    tokio::spawn(async move {
        let mut heartbeat_timer = interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                result = framed_reader.next() => {
                    match result {
                        Some(Ok(Message::Chat(msg))) => {
                            let _ = net_notifier.send(ChatEvent::MessageReceived(msg));
                        }
                        Some(Ok(Message::Error(err))) => {
                            let _ = net_notifier.send(ChatEvent::Error(format!("Server Error: {}", err)));
                        }
                        None => {
                            let _ = net_notifier.send(
                                ChatEvent::SystemMessage(
                                    ChatPacket::new_server_packet("Connection closed by server.".to_string())));
                            break;
                        }
                        _ => {}
                    }
                }
                Some(msg) = network_rx.recv() => {
                    if let Err(e) = client.writer.send(Message::Chat(msg)).await {
                        let _ = net_notifier.send(ChatEvent::Error(format!("Failed to send message: {}", e)));
                    }
                }
                _ = heartbeat_timer.tick() => {
                    if let Err(e) = client.writer.send(Message::Heartbeat).await {
                            let _ = net_notifier.send(ChatEvent::Error(format!("Heartbeat failed: {}", e)));
                    }
                }
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let term_backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(term_backend)?;

    let mut app = ChatApp::new(username.clone(), ui_tx);

    loop {
        terminal.draw(|f| app.update_ui(f))?;

        while let Ok(event) = ui_rx.try_recv() {
            match event {
                ChatEvent::MessageReceived(msg) => {
                    app.messages.push(msg);
                    app.scroll = app.scroll.saturating_add(1);
                }
                ChatEvent::SystemMessage(msg) => {
                    app.messages.push(msg);
                    app.scroll = app.scroll.saturating_add(1);
                }
                ChatEvent::Error(err) => {
                    app.messages
                        .push(ChatPacket::new_server_packet(format!("ERROR: {}", err)));
                    app.scroll = app.scroll.saturating_add(1);
                }
            }
        }

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Enter => app.submit_message(),
                KeyCode::Char(c) => app.input.push(c),
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Up => {
                    app.scroll = app.scroll.saturating_sub(1);
                }
                KeyCode::Down => {
                    if app.scroll < app.scroll_limit {
                        app.scroll = app.scroll.saturating_add(1);
                    }
                }
                KeyCode::Esc => break,
                _ => {}
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
