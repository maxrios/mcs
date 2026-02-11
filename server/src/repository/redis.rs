use super::PresenceRepository;
use crate::error::Result;
use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use protocol::Message;
use redis::Client;
use tokio::sync::broadcast::Sender;
use tracing::error;

#[derive(Clone)]
pub struct RedisRepository {
    conn: redis::aio::MultiplexedConnection,
}

impl RedisRepository {
    pub async fn new(url: &str, app_sender: Sender<Message>) -> Result<Self> {
        let client = Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;

        Self::spawn_subscriber(client.clone(), app_sender);

        Ok(Self { conn })
    }

    fn spawn_subscriber(client: Client, sender: Sender<Message>) {
        tokio::spawn(async move {
            let mut conn = match client.get_async_pubsub().await {
                Ok(c) => c,
                Err(e) => {
                    error!(err = ?e, "redis pubsub connect failed");
                    return;
                }
            };

            if let Err(e) = conn.subscribe("mcs:chat").await {
                error!(err = ?e, "failed to subscribe to 'mcs:chat'");
                return;
            }

            let mut stream = conn.on_message();
            while let Some(msg) = stream.next().await {
                let payload: Vec<u8> = match msg.get_payload() {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                if let Ok(chat_msg) = postcard::from_bytes::<Message>(&payload) {
                    let _ = sender.send(chat_msg);
                }
            }
        });
    }
}

#[async_trait]
impl PresenceRepository for RedisRepository {
    async fn set_online(&self, username: &str) -> Result<bool> {
        let key = format!("user:session:{username}");
        let mut conn = self.conn.clone();

        let res: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg("online")
            .arg("NX")
            .arg("EX")
            .arg(30)
            .query_async(&mut conn)
            .await?;

        Ok(res.is_some())
    }

    async fn set_offline(&self, username: &str) -> Result<()> {
        let key = format!("user:session:{username}");
        let mut conn = self.conn.clone();
        redis::cmd("DEL")
            .arg(key)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(())
    }

    async fn refresh_heartbeat(&self, username: &str) -> Result<()> {
        let key = format!("user:session:{username}");
        let mut conn = self.conn.clone();
        redis::cmd("EXPIRE")
            .arg(key)
            .arg(30)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(())
    }

    async fn register_node(&self, address: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        let timestamp = Utc::now().timestamp();

        redis::cmd("ZADD")
            .arg("mcs:node")
            .arg(timestamp)
            .arg(address)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(())
    }

    async fn broadcast(&self, msg: Message) -> Result<()> {
        let payload = postcard::to_stdvec(&msg)?;
        let mut conn = self.conn.clone();
        redis::cmd("PUBLISH")
            .arg("mcs:chat")
            .arg(payload)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(())
    }
}
