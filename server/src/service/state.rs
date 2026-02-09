use crate::error::Result;
use crate::repository::{postgres::PostgresRepository, redis::RedisRepository};
use crate::service::{AuthService, ChatService, NodeService};
use protocol::Message;
use std::sync::Arc;
use tokio::sync::broadcast::{self, Sender};

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthService>,
    pub chat: Arc<ChatService>,
    pub node: Arc<NodeService>,
    pub internal_broadcast_tx: Sender<Message>,
}

impl AppState {
    pub async fn new(db_url: &str, redis_url: &str, node_id: String) -> Result<Self> {
        let (tx, _) = broadcast::channel(100);
        let pg_repo = Arc::new(PostgresRepository::new(db_url).await?);
        let redis_repo = Arc::new(RedisRepository::new(redis_url, tx.clone()).await?);

        let auth_service = Arc::new(AuthService::new(pg_repo.clone(), redis_repo.clone()));
        let chat_service = Arc::new(ChatService::new(pg_repo.clone(), redis_repo.clone()));
        let node_service = Arc::new(NodeService::new(redis_repo, node_id));

        Ok(Self {
            auth: auth_service,
            chat: chat_service,
            node: node_service,
            internal_broadcast_tx: tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.internal_broadcast_tx.subscribe()
    }
}
