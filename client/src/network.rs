use std::{fs::File, io::BufReader, sync::Arc};

use futures::{SinkExt, StreamExt};
use protocol::{McsCodec, Message};
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::ServerName;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_rustls::TlsConnector;
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::{
    error::{Error, Result},
    event::AppEvent,
};

/// A client to handle network events.
pub struct NetworkClient {
    /// Channel to send messages to the server.
    tx: mpsc::UnboundedSender<Message>,
}

impl NetworkClient {
    pub const fn new(tx: mpsc::UnboundedSender<Message>) -> Self {
        Self { tx }
    }

    pub fn send(&self, msg: Message) -> Result<()> {
        self.tx.send(msg).map_err(|_| Error::ChannelClosed)
    }

    pub async fn connect(ip: &str, event_tx: mpsc::UnboundedSender<AppEvent>) -> Result<Self> {
        let mut root_store = RootCertStore::empty();
        let file = File::open("tls/ca.cert")?;
        let mut reader = BufReader::new(file);

        for cert in rustls_pemfile::certs(&mut reader) {
            let cert = cert.map_err(|e| Error::Cert(e.to_string()))?;
            root_store
                .add(cert)
                .map_err(|e| Error::Tls(e.to_string()))?;
        }

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));

        let stream = TcpStream::connect(format!("{ip}:64400"))
            .await
            .map_err(|e| Error::Connect(e.to_string()))?;

        let domain = ServerName::try_from(ip.to_string())
            .map_err(|e| Error::Tls(format!("Invalid DNS name: {e}")))?;

        let tls_stream = connector
            .connect(domain, stream)
            .await
            .map_err(|e| Error::Tls(e.to_string()))?;

        let (reader, writer) = tokio::io::split(tls_stream);
        let mut framed_reader = FramedRead::new(reader, McsCodec);
        let mut framed_writer = FramedWrite::new(writer, McsCodec);

        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();

        tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if framed_writer.send(msg).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            while let Some(Ok(msg)) = framed_reader.next().await {
                if event_tx.send(AppEvent::Network(msg)).is_err() {
                    break;
                }
            }
            let _ = event_tx.send(AppEvent::Err(Error::Disconnected));
        });

        Ok(Self::new(outbound_tx))
    }

    pub fn into_inner(self) -> mpsc::UnboundedSender<Message> {
        self.tx
    }
}
