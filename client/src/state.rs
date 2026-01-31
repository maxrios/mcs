use chrono::{
    Local, TimeZone, Utc,
    format::{DelayedFormat, StrftimeItems},
};
use futures::SinkExt;
use protocol::{ChatError, ChatPacket, McsCodec, Message};
use ratatui::style::Color;
use tokio::io::AsyncWrite;
use tokio_util::codec::FramedWrite;

type MessageWriter<W> = FramedWrite<W, McsCodec>;

pub enum ChatEvent {
    UserMessage(ChatPacket),
    SystemMessage(ChatPacket),
    Error(String),
}

impl ChatEvent {
    pub fn to_colored_string(&self) -> Option<(String, Color)> {
        match self {
            Self::UserMessage(msg) => Some((
                format!(
                    "[{}] {}: {}",
                    Self::format_time(msg.timestamp)?,
                    msg.sender,
                    msg.content
                ),
                Color::White,
            )),
            Self::SystemMessage(msg) => Some((
                format!("[{}] {}", Self::format_time(msg.timestamp)?, msg.content),
                Color::Gray,
            )),
            Self::Error(err) => Some((err.clone(), Color::Red)),
        }
    }

    fn format_time<'a>(timestamp: i64) -> Option<DelayedFormat<StrftimeItems<'a>>> {
        let utc_datetime = Utc.timestamp_opt(timestamp, 0).single()?;
        let local_datetime = utc_datetime.with_timezone(&Local);
        Some(local_datetime.format("%D %l:%M %p"))
    }
}

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
        }

        Ok(())
    }
}
