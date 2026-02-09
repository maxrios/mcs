use crate::error::Result;
use crate::repository::{MessageRepository, PresenceRepository};
use protocol::ChatPacket;
use protocol::Message;
use std::sync::Arc;

#[derive(Clone)]
pub struct ChatService {
    messages: Arc<dyn MessageRepository>,
    presence: Arc<dyn PresenceRepository>,
}

impl ChatService {
    pub fn new(
        messages: Arc<dyn MessageRepository>,
        presence: Arc<dyn PresenceRepository>,
    ) -> Self {
        Self { messages, presence }
    }

    pub async fn broadcast_user_message(&self, sender: &str, content: String) -> Result<()> {
        let packet = ChatPacket::new_user_packet(sender.to_string(), content);

        self.messages.save_message(&packet).await?;
        self.presence.broadcast(Message::Chat(packet)).await?;

        Ok(())
    }

    pub async fn broadcast_system_message(&self, content: String) -> Result<ChatPacket> {
        let packet = ChatPacket::new_server_packet(content);

        self.messages.save_message(&packet).await?;
        self.presence
            .broadcast(Message::Chat(packet.clone()))
            .await?;

        Ok(packet)
    }

    pub async fn get_history(&self, before_ts: i64) -> Result<Vec<ChatPacket>> {
        self.messages.get_recent_messages(before_ts).await
    }
}
