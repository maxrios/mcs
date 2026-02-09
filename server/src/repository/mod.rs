use crate::error::Result;
use async_trait::async_trait;
use protocol::{ChatPacket, Message};

pub mod postgres;
pub mod redis;

/// Manages persistent user data.
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create_user(&self, username: &str, password: &str) -> Result<()>;
    async fn verify_credentials(&self, username: &str, password: &str) -> Result<bool>;
}

/// Manages persistent message history.
#[async_trait]
pub trait MessageRepository: Send + Sync {
    async fn save_message(&self, msg: &ChatPacket) -> Result<()>;
    async fn get_recent_messages(&self, before_ts: i64) -> Result<Vec<ChatPacket>>;
}

/// Manages ephemeral states.
#[async_trait]
pub trait PresenceRepository: Send + Sync {
    async fn set_online(&self, username: &str) -> Result<bool>;
    async fn set_offline(&self, username: &str) -> Result<()>;
    async fn refresh_heartbeat(&self, username: &str) -> Result<()>;
    async fn register_node(&self, address: &str) -> Result<()>;
    async fn broadcast(&self, msg: Message) -> Result<()>;
}
