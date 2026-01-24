use futures::SinkExt;
use protocol::{ChatError, McsCodec, Message};
use tokio::io::AsyncWrite;
use tokio_util::codec::FramedWrite;

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

    pub async fn connect(&mut self) -> Result<(), ChatError> {
        if self
            .writer
            .send(Message::Join(self.username.clone()))
            .await
            .is_err()
        {
            return Err(ChatError::Network);
        };

        Ok(())
    }
}
