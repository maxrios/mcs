use futures::StreamExt;
use protocol::Message;
use redis::Client;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::error::Result;

#[derive(Clone)]
pub struct Redis {
    conn: redis::aio::MultiplexedConnection,
}

impl Redis {
    pub async fn new(redis_url: &str, sender: &Sender<Message>) -> Result<Self> {
        let client = Client::open(redis_url)?;
        let conn = client.get_multiplexed_async_connection().await?;

        Redis::spawn_pubsub_task(sender.clone(), client.clone());

        Ok(Self { conn })
    }

    fn spawn_pubsub_task(sender: Sender<Message>, client: Client) {
        tokio::spawn(async move {
            let mut conn = match client.get_async_pubsub().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!(%e, "failed to subscribe to chat channel");
                    return;
                }
            };

            if let Err(e) = conn.subscribe("mcs:chat").await {
                error!(%e, "failed to subscribe to chat channel");
                return;
            }

            let mut stream = conn.on_message();
            while let Some(payload) = stream.next().await {
                if let Ok(msg) = postcard::from_bytes::<Message>(payload.get_payload_bytes()) {
                    let _ = sender.send(msg);
                }
            }
        });
    }

    pub async fn publish(&self, msg: Message) -> Result<()> {
        let payload = postcard::to_stdvec(&msg)?;
        let mut conn = self.conn.clone();
        Ok(redis::cmd("PUBLISH")
            .arg("mcs:chat")
            .arg(payload)
            .query_async::<()>(&mut conn)
            .await?)
    }

    pub async fn set(&self, name: &str) -> Result<bool> {
        let key = format!("user:session:{}", name);
        let mut conn = self.conn.clone();
        Ok(redis::cmd("SET")
            .arg(&key)
            .arg("online")
            .arg("NX")
            .arg("EX")
            .arg(30)
            .query_async(&mut conn)
            .await?)
    }

    pub async fn del(&self, name: &str) -> Result<()> {
        let key = format!("user:session:{}", name);
        let mut conn = self.conn.clone();
        Ok(redis::cmd("DEL").arg(key).query_async(&mut conn).await?)
    }

    pub async fn expire(&self, name: &str) -> Result<()> {
        let key = format!("user:session:{}", name);
        let mut conn = self.conn.clone();
        Ok(redis::cmd("EXPIRE")
            .arg(&key)
            .arg(30)
            .query_async(&mut conn)
            .await?)
    }
}
