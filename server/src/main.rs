use std::{fs::File, io::BufReader, sync::Arc};

use futures::StreamExt;
use protocol::{McsCodec, Message};
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::FramedRead;

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

    let server = Arc::new(ChatServer::new(host));
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

            if let Some(Ok(Message::Join(name))) = framed_reader.next().await
                && server_ref.register_user(&name, writer).await.is_ok()
            {
                server_ref
                    .broadcast("server", format!("{} joined.\n", name).as_str())
                    .await;

                handle_session(&name, framed_reader, server_ref).await;
            }
        });
    }
}

async fn handle_session<R, W>(
    name: &str,
    mut reader: FramedRead<R, McsCodec>,
    server: Arc<ChatServer<W>>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + Sync + 'static,
{
    while let Some(Ok(msg)) = reader.next().await {
        match msg {
            Message::Chat(text) => server.broadcast(name, format!("{}\n", text).as_str()).await,
            Message::Heartbeat => {
                if !server.heartbeat(name).await {
                    break;
                }
            }
            _ => {}
        }
    }
    server.remove_user(name).await;
    server
        .broadcast("server", format!("{} left.\n", name).as_str())
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
