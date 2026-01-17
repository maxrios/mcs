use std::{fs::File, io::BufReader, sync::Arc};

use futures::{SinkExt, StreamExt};
use protocol::{ChatPacket, McsCodec, Message};
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::{FramedRead, FramedWrite};

use tokio::{
    io::{AsyncRead, AsyncWrite, split},
    net::TcpListener,
};

mod state;
use state::ChatServer;

#[tokio::main]
async fn main() {
    let host = "127.0.0.1:64400";
    let certs = load_certs("tls/server.cert");
    let keys = load_keys("tls/server.key");
    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, keys)
        .expect("bad certificate or key");
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    let server = Arc::new(ChatServer::new());
    server.clone().spawn_reaper();

    let listener = TcpListener::bind(host).await.unwrap();

    println!("Server running on {}", host);

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let acceptor = acceptor.clone();
        let server_ref = Arc::clone(&server);

        tokio::spawn(async move {
            let stream = match acceptor.accept(socket).await {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("TLS handshake failed: {}", e);
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
                handle_session(&name, framed_reader, framed_writer, server_ref).await;
            }
        });
    }
}

async fn handle_registration<W>(
    server: &Arc<ChatServer>,
    writer: &mut FramedWrite<W, McsCodec>,
    name: &str,
) -> Result<(), ()>
where
    W: AsyncWrite + Unpin + Send + Sync + 'static,
{
    match server.register_user(name).await {
        Ok(_) => {
            let _ = writer
                .send(Message::Chat(ChatPacket::new_server_packet(
                    "Connected!".to_string(),
                )))
                .await;

            server
                .broadcast(ChatPacket::new_server_packet(format!("{} joined.\n", name)))
                .await;

            let history = server.get_history().await;
            for packet in history {
                let _ = writer.send(Message::Chat(packet)).await;
            }

            Ok(())
        }
        Err(e) => {
            let _ = writer.send(Message::Error(e)).await;
            Err(())
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
                        Message::Chat(text) => server.broadcast(text).await,

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
    server
        .broadcast(ChatPacket::new_server_packet(format!("{} left.\n", name)))
        .await;
}

fn load_certs(path: &str) -> Vec<CertificateDer<'static>> {
    let file = File::open(path).expect("Failed to open cert path");
    let mut reader = BufReader::new(file);
    certs(&mut reader).map(|result| result.unwrap()).collect()
}

fn load_keys(path: &str) -> PrivateKeyDer<'static> {
    let file = File::open(path).expect("cannot open key file");
    let mut reader = BufReader::new(file);
    // Assuming PKCS8 (standard format)
    pkcs8_private_keys(&mut reader)
        .next()
        .unwrap()
        .unwrap()
        .into()
}
