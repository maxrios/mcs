use crate::error::Result;
use crate::repository::PresenceRepository;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info};

#[derive(Clone)]
pub struct NodeService {
    presence: Arc<dyn PresenceRepository>,
    node_id: String,
}

impl NodeService {
    pub fn new(presence: Arc<dyn PresenceRepository>, node_id: String) -> Self {
        Self { presence, node_id }
    }

    pub async fn register(&self) -> Result<()> {
        info!(node_id=%self.node_id, "registering node");
        self.presence.register_node(&self.node_id).await
    }

    pub fn start_heartbeat(&self) {
        let presence = self.presence.clone();
        let node_id = self.node_id.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(3));

            loop {
                interval.tick().await;
                if let Err(e) = presence.register_node(&node_id).await {
                    error!(node_id=%node_id, err=?e, "heatbeat failed");
                }
            }
        });
    }
}
