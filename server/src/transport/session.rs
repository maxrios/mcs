use std::time::Duration;

use crate::service::AppState;
use futures::{SinkExt, StreamExt};
use protocol::{McsCodec, Message};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf},
    sync::broadcast::Receiver,
};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{error, warn};

pub struct ClientSession<S> {
    username: String,
    state: AppState,
    reader: FramedRead<ReadHalf<S>, McsCodec>,
    writer: FramedWrite<WriteHalf<S>, McsCodec>,
    rx: Receiver<Message>,
}

impl<S> ClientSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Sync + Send + 'static,
{
    pub fn new(
        username: String,
        state: AppState,
        reader: FramedRead<ReadHalf<S>, McsCodec>,
        writer: FramedWrite<WriteHalf<S>, McsCodec>,
    ) -> Self {
        let rx = state.subscribe();
        Self {
            username,
            state,
            reader,
            writer,
            rx,
        }
    }

    pub async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                result = self.reader.next() => {
                    match result {
                        Some(Ok(msg)) => self.handle_client_message(msg).await,
                        Some(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Some(Err(e)) => {
                            error!(user=%self.username, err=?e, "failed to decode message");
                            break;
                        }
                        None => break,
                    }
                }

                Ok(msg) = self.rx.recv() => {
                    if let Err(e) = self.writer.send(msg).await {
                        error!(user=%self.username, err=?e, "failed to send broadcast to client");
                        break;
                    }
                }

                _ = interval.tick() => {
                    if let Err(e) = self.state.auth.refresh_session(&self.username).await {
                        error!(user=%self.username, err=?e, "failed to refresh session");
                        break;
                    }
                }
            }
        }

        self.disconnect().await;
    }

    async fn handle_client_message(&mut self, msg: Message) {
        match msg {
            Message::Chat(packet) => {
                if let Err(e) = self
                    .state
                    .chat
                    .broadcast_user_message(&self.username, packet.content)
                    .await
                {
                    error!(user=%self.username, err=?e, "failed to broadcast message");
                    let _ = self.writer.send(Message::Error(e.to_chat_error())).await;
                }
            }
            Message::HistoryRequest(ts) => match self.state.chat.get_history(ts).await {
                Ok(history) => {
                    let _ = self.writer.send(Message::HistoryResponse(history)).await;
                }
                Err(e) => {
                    warn!(user=%self.username, err=?e, timestamp=%ts, "failed to provide history");
                    let _ = self.writer.send(Message::Error(e.to_chat_error())).await;
                }
            },
            Message::Heartbeat => {
                let _ = self.state.auth.refresh_session(&self.username).await;
            }
            _ => {}
        }
    }

    async fn disconnect(&self) {
        if let Err(e) = self.state.auth.logout(&self.username).await {
            error!(user=%self.username, err=?e, "failed to clear session");
        }

        let _ = self
            .state
            .chat
            .broadcast_system_message(format!("{} left.\n", self.username))
            .await;
    }
}
