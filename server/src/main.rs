use std::{env, fs::File, io::BufReader, sync::Arc};

use futures::{SinkExt, StreamExt};
use protocol::{ChatPacket, McsCodec, Message};
use rustls::{ServerConfig, crypto::ring};
use rustls_pemfile::{certs, pkcs8_private_keys};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{error, info, warn};
use tracing_subscriber::util::SubscriberInitExt;

use tokio::{
    io::{AsyncRead, AsyncWrite, split},
    net::TcpListener,
};

mod db;
mod error;
mod state;
use state::ChatServer;
use tracing_subscriber::layer::SubscriberExt;

use crate::error::{Error, Result};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if ring::default_provider().install_default().is_err() {
        panic!("failed to set default CryptoProvider");
    }

    let host = "0.0.0.0:64400";
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/postgres".to_string());

    let certs = match load_certs("tls/server.cert") {
        Ok(certs) => certs,
        Err(e) => {
            error!(%e, "failed to load certs");
            return;
        }
    };
    let keys = match load_keys("tls/server.key") {
        Ok(keys) => keys,
        Err(e) => {
            error!(%e, "failed to load keys");
            return;
        }
    };
    let tls_config = match ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, keys)
    {
        Ok(config) => config,
        Err(e) => {
            error!(%e, "failed to set single cert and match private keys");
            return;
        }
    };

    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    let server = match ChatServer::new(&database_url).await {
        Ok(server) => Arc::new(server),
        Err(e) => {
            error!(%e, "failed to initilize database");
            return;
        }
    };
    server.clone().spawn_watchdog();

    let listener = match TcpListener::bind(host).await {
        Ok(listener) => {
            info!(%host, "server running");
            listener
        }
        Err(e) => {
            error!(%e, "server failed to bind to host");
            return;
        }
    };

    loop {
        let (socket, addr) = match listener.accept().await {
            Ok((socket, addr)) => (socket, addr),
            Err(e) => {
                warn!(%e, "connection failed");
                continue;
            }
        };

        let acceptor = acceptor.clone();
        let server_ref = Arc::clone(&server);

        tokio::spawn(async move {
            let stream = match acceptor.accept(socket).await {
                Ok(stream) => stream,
                Err(e) => {
                    error!(%e, "TLS handshake failed");
                    return;
                }
            };

            let (reader, writer) = split(stream);

            let mut framed_reader = FramedRead::new(reader, McsCodec);
            let mut framed_writer = FramedWrite::new(writer, McsCodec);

            if let Some(Ok(Message::Join(name))) = framed_reader.next().await
                && handle_registration(&server_ref, &mut framed_writer, &name)
                    .await
                    .is_ok()
            {
                info!(ip = %addr.ip(), port = %addr.port(), name = %name, "connected user");
                handle_session(&name, framed_reader, framed_writer, server_ref).await;
            }
        });
    }
}

async fn handle_registration<W>(
    server: &Arc<ChatServer>,
    writer: &mut FramedWrite<W, McsCodec>,
    name: &str,
) -> Result<()>
where
    W: AsyncWrite + Unpin + Send + Sync + 'static,
{
    match server.register_user(name).await {
        Ok(_) => {
            server
                .broadcast(ChatPacket::new_server_packet(format!("{} joined.\n", name)))
                .await?;

            let history = server.get_history().await?;

            for packet in history {
                let _ = writer.send(Message::Chat(packet)).await;
            }

            Ok(())
        }
        Err(e) => {
            let _ = writer.send(Message::Error(e.to_chat_error())).await;
            Err(e)
        }
    }
}

async fn handle_session<R, W>(
    name: &str,
    mut reader: FramedRead<R, McsCodec>,
    mut writer: FramedWrite<W, McsCodec>,
    server: Arc<ChatServer>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut rx = server.subscribe();

    loop {
        tokio::select! {
            result = reader.next() => {
                match result {
                    Some(Ok(msg)) => match msg {
                        Message::Chat(text) => {
                            if let Err(e) = server.broadcast(text).await {
                                if let Err(e2) = writer.send(Message::Error(e.to_chat_error())).await {
                                    warn!(%e2, "failed to notify user of error");
                                }
                                error!(%e, "broadcast error");
                            }
                        },
                        Message::Heartbeat => {
                            if !server.heartbeat(name).await {
                                break;
                            }
                        }
                        _ => break
                    }
                    _ => break
                }
            }
            Ok(msg) = rx.recv() => {
                if writer.send(msg).await.is_err() {
                    break;
                }
            }
        }
    }
    server.remove_user(name).await;
    if let Err(e) = server
        .broadcast(ChatPacket::new_server_packet(format!("{} left.\n", name)))
        .await
    {
        error!(%e, "failed to broadcast user leave");
    }
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(Error::IO(e)),
    };
    let mut reader = BufReader::new(file);
    Ok(certs(&mut reader).map(|result| result.unwrap()).collect())
}

fn load_keys(path: &str) -> Result<PrivateKeyDer<'static>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(Error::IO(e)),
    };
    let mut reader = BufReader::new(file);
    Ok(pkcs8_private_keys(&mut reader)
        .next()
        .unwrap()
        .unwrap()
        .into())
}
