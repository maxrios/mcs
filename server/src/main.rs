#![warn(clippy::all, clippy::pedantic, clippy::nursery, unused_extern_crates)]

use futures::{SinkExt, StreamExt};
use protocol::{ChatPacket, JoinPacket, McsCodec, Message};
use tokio::{io::split, net::TcpListener};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod repository;
mod service;
mod transport;

use config::Config;
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

        let state = state.clone();

        tokio::spawn(async move {
            let (reader, writer) = split(socket);
            let mut framed_reader = FramedRead::new(reader, McsCodec);
            let mut framed_writer = FramedWrite::new(writer, McsCodec);

            match framed_reader.next().await {
                // 1. Success: User sent a Join Packet
                Some(Ok(Message::Join(JoinPacket { username, password }))) => {
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
                                    let _ =
                                        framed_writer.send(Message::HistoryResponse(history)).await;
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
                }
                // 2. Health Check: Connection closed immediately (0 bytes)
                None => {
                    // This is normal behavior for the Load Balancer's health check.
                    // We use 'debug!' so it doesn't spam your console logs.
                    tracing::debug!(ip = %addr.ip(), "health check probe (connection closed)");
                }
                // 3. Actual Protocol Violation: User sent Chat/Heartbeat BEFORE Joining
                Some(Ok(msg)) => {
                    warn!(ip = %addr.ip(), ?msg, "protocol violation: expected JoinPacket, got {:?}", msg);
                }
                // 4. Decode Error
                Some(Err(e)) => {
                    warn!(ip = %addr.ip(), err = ?e, "failed to decode packet");
                }
            }
        });
    }
}
