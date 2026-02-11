use metrics::gauge;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct BackendState {
    pub addr: String,
    pub active_connections: usize,
    pub is_healthy: bool,
}

#[derive(Clone)]
pub struct LbState {
    backends: Arc<RwLock<HashMap<String, BackendState>>>,
}

impl LbState {
    pub fn new() -> Self {
        Self {
            backends: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn next_backend(&self) -> Option<String> {
        let backends = self.backends.read().await;
        backends
            .values()
            .filter(|b| b.is_healthy)
            .min_by_key(|b| b.active_connections)
            .map(|b| b.addr.clone())
    }

    pub async fn add_backend(&self, addr: String, active_connections: usize) {
        let mut lock = self.backends.write().await;
        lock.insert(
            addr.clone(),
            BackendState {
                addr,
                active_connections,
                is_healthy: true,
            },
        );

        gauge!("lb_healthy_backends").set(lock.len() as f64)
    }

    pub async fn remove_backend(&self, addr: &str) {
        let mut lock = self.backends.write().await;
        lock.remove(addr);
        gauge!("lb_healthy_backends").set(lock.len() as f64)
    }

    pub async fn get_backend_addrs(&self) -> Vec<String> {
        self.backends.read().await.keys().cloned().collect()
    }

    pub async fn set_health(&self, addr: &str, is_healthy: bool) {
        if let Some(b) = self.backends.write().await.get_mut(addr) {
            b.is_healthy = is_healthy;
        }
    }

    pub async fn inc_backend_connection(&self, addr: &str) {
        if let Some(b) = self.backends.write().await.get_mut(addr) {
            b.active_connections += 1;
            gauge!("lb_backend_active_connections", "backend" => addr.to_string())
                .set(b.active_connections as f64);
            gauge!("lb_active_connections").increment(1);
        }
    }

    pub async fn dec_backend_connection(&self, addr: &str) {
        if let Some(b) = self.backends.write().await.get_mut(addr) {
            b.active_connections -= 1;
            gauge!("lb_backend_active_connections", "backend" => addr.to_string())
                .set(b.active_connections as f64);
            gauge!("lb_active_connections").decrement(1);
        }
    }
}
