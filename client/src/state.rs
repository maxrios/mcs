use futures::{SinkExt, StreamExt};
use protocol::{ChatError, McsCodec, Message};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite};

type MessageWriter<W> = FramedWrite<W, McsCodec>;

pub struct ChatClient<W> {
    pub writer: MessageWriter<W>,
    pub username: String,
}

impl<W: AsyncWrite + Unpin> ChatClient<W> {
    pub fn new(writer: W, username: String) -> Self {
        Self {
            writer: FramedWrite::new(writer, McsCodec),
            username,
        }
    }

    pub async fn connect<R>(
        &mut self,
        reader: &mut FramedRead<R, McsCodec>,
    ) -> Result<(), ChatError>
    where
        R: AsyncRead + Unpin,
    {
        let _ = self.writer.send(Message::Join(self.username.clone())).await;

        match reader.next().await {
            Some(Ok(Message::Chat(msg))) => println!("{}", msg.content),
            Some(Ok(Message::Error(err))) => return Err(err),
            _ => println!("Connection closed by server during join."),
        }

        Ok(())
    }
}
