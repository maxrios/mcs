use futures::{SinkExt, StreamExt};
use protocol::{McsCodec, Message};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio_util::codec::{FramedRead, FramedWrite};

type MessageWriter = FramedWrite<OwnedWriteHalf, McsCodec>;

pub struct ChatClient {
    pub writer: MessageWriter,
    pub username: String,
}

impl ChatClient {
    pub fn new(writer: OwnedWriteHalf, username: String) -> Self {
        Self {
            writer: FramedWrite::new(writer, McsCodec),
            username,
        }
    }

    pub async fn connect(&mut self, reader: &mut FramedRead<OwnedReadHalf, McsCodec>) {
        let _ = self.writer.send(Message::Join(self.username.clone())).await;

        match reader.next().await {
            Some(Ok(Message::Chat(msg))) => println!("{}", msg),
            Some(Ok(Message::Error(err))) => println!("Server rejected join: {:?}", err),
            _ => println!("Connection closed by server during join."),
        }
    }
}
