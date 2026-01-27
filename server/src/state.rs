use crate::{
    db::Database,
    error::{Error, Result},
    redis::Redis,
};

use protocol::{ChatPacket, Message};
use tokio::sync::broadcast::{self};

pub struct ChatServer {
    channel_tx: broadcast::Sender<Message>,
    redis: Redis,
    db: Database,
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

    pub async fn get_history(&self) -> Result<Vec<ChatPacket>> {
        Ok(self.db.get_recent_messages().await?)
    }

    pub async fn broadcast(&self, msg: ChatPacket) -> Result<()> {
        self.db.save_message(&msg).await?;
        self.redis.publish(Message::Chat(msg)).await
    }

    pub async fn register_user(&self, name: &str) -> Result<()> {
        if name.len() < 3 {
            return Err(Error::UsernameTooShort(name.to_string()));
        }

        if !self.redis.set(name).await? || name == "server" {
            return Err(Error::UsernameTaken(name.to_string()));
        }

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) -> Result<()> {
        self.redis.del(name).await
    }

    pub async fn heartbeat(&self, name: &str) -> Result<()> {
        self.redis.expire(name).await
    }
}
