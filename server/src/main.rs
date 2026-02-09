#![warn(clippy::all, clippy::pedantic, clippy::nursery, unused_extern_crates)]

use futures::{SinkExt, StreamExt};
use protocol::{ChatPacket, JoinPacket, McsCodec, Message};
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::{fs::File, io::BufReader, sync::Arc};
use tokio::{io::split, net::TcpListener};
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod repository;
mod service;
mod transport;

use config::Config;
use error::Error;
use service::AppState;
use transport::session::ClientSession;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::load();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls_config = load_tls_config(&config.tls_cert_path, &config.tls_key_path)?;
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let addr = format!("{}:{}", config.hostname, config.port);
    let state: AppState = AppState::new(&config.db_url, &config.redis_url, addr.clone()).await?;
    state.node.register().await?;
    state.node.start_heartbeat();

    let listener = TcpListener::bind(&addr).await?;
    info!(%addr, "server running");

    loop {
        let (socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!(err=?e, "failed to accept new connection");
                continue;
            }
        };

        let acceptor = acceptor.clone();
        let state = state.clone();

        tokio::spawn(async move {
            info!(ip=%addr.ip(), "accepting new connection");

            let stream = match acceptor.accept(socket).await {
                Ok(s) => s,
                Err(e) => {
                    warn!(ip=%addr.ip(), err=?e, "TLS handshake failed");
                    return;
                }
            };

            let (reader, writer) = split(stream);
            let mut framed_reader = FramedRead::new(reader, McsCodec);
            let mut framed_writer = FramedWrite::new(writer, McsCodec);
            if let Some(Ok(Message::Join(JoinPacket { username, password }))) =
                framed_reader.next().await
            {
                match state.auth.register_and_login(&username, &password).await {
                    Ok(()) => {
                        info!(user=%username, "user authenticated");

                        let join_msg = match state
                            .chat
                            .broadcast_system_message(format!("{username} joined.\n"))
                            .await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                warn!(err=?e, "failed to broadcast join message");
                                ChatPacket::new_server_packet(String::new())
                            }
                        };

                        match state.chat.get_history(join_msg.timestamp + 1).await {
                            Ok(history) => {
                                let _ = framed_writer.send(Message::HistoryResponse(history)).await;
                            }
                            Err(e) => {
                                error!(err=?e, "failed to fetch history during join");
                            }
                        }

                        let mut session =
                            ClientSession::new(username, state, framed_reader, framed_writer);
                        session.run().await;
                    }
                    Err(e) => {
                        warn!(user=%username, err=?e, "failed to authenticate user");
                        let _ = framed_writer.send(Message::Error(e.to_chat_error())).await;
                    }
                }
            } else {
                warn!(ip=%addr.ip(), "protocol violation: expected join packet");
            }
        });
    }
}

fn load_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig, Error> {
    let cert_file = File::open(cert_path)?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs = certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

    let key_file = File::open(key_path)?;
    let mut key_reader = BufReader::new(key_file);
    let key = pkcs8_private_keys(&mut key_reader).next().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "no private key found")
    })??;

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key.into())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e).into())
}
