use crate::{
    db::Database,
    error::{Error, Result},
    redis::Redis,
};

use protocol::{ChatPacket, Message};
use tokio::sync::broadcast::{self};

pub struct ChatServer {
    channel_tx: broadcast::Sender<Message>,
    pub redis: Redis,
    pub db: Database,
}

impl ChatServer {
    pub async fn new(database_url: &str, redis_url: &str) -> Result<Self> {
        let (tx, _) = broadcast::channel(100);
        let db = Database::new(database_url).await?;
        let redis = Redis::new(redis_url, &tx).await?;

        Ok(Self {
            channel_tx: tx,
            redis,
            db,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.channel_tx.subscribe()
    }

    pub async fn get_history(&self, timestamp: i64) -> Result<Vec<ChatPacket>> {
        self.db.get_recent_messages(timestamp).await
    }

    pub async fn broadcast(&self, msg: ChatPacket) -> Result<()> {
        self.db.save_message(&msg).await?;
        self.redis.publish_message(Message::Chat(msg)).await
    }

    pub async fn register_user(&self, username: &str, password: &str) -> Result<()> {
        if username.len() < 3 {
            return Err(Error::UsernameTooShort(username.to_string()));
        }

        let is_valid = self.db.verify_credentials(username, password).await?;

        if !is_valid {
            return Err(Error::InvalidCredentials);
        }

        if !self.redis.set_connection(username).await? {
            return Err(Error::UsernameTaken("user already logged in".to_string()));
        }

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) -> Result<()> {
        self.redis.del_connection(name).await
    }

    pub async fn heartbeat(&self, name: &str) -> Result<()> {
        self.redis.expire_connection(name).await
    }
}
