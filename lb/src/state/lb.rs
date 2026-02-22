use crate::state::ClientState;
use dashmap::DashMap;
use governor::Quota;
use metrics::gauge;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BackendState {
    pub addr: String,
    pub active_connections: usize,
    pub is_healthy: bool,
}

#[derive(Clone)]
pub struct LoadBalancerState {
    backends: Arc<DashMap<String, BackendState>>,
    pub clients: Arc<DashMap<IpAddr, Arc<ClientState>>>,
}

impl LoadBalancerState {
    pub fn new() -> Self {
        Self {
            backends: Arc::new(DashMap::new()),
            clients: Arc::new(DashMap::new()),
        }
    }

    pub async fn next_backend(&self) -> Option<String> {
        self.backends
            .iter()
            .filter(|b| b.is_healthy)
            .min_by_key(|b| b.active_connections)
            .map(|b| b.addr.clone())
    }

    pub async fn add_backend(&self, addr: String, active_connections: usize) {
        self.backends.insert(
            addr.clone(),
            BackendState {
                addr,
                active_connections,
                is_healthy: true,
            },
        );

        gauge!("lb_healthy_backends").set(self.backends.len() as f64)
    }

    pub async fn remove_backend(&self, addr: &str) {
        gauge!("lb_healthy_backends").set(self.backends.len() as f64);
        self.backends.remove(addr);
    }

    pub async fn get_backend_addrs(&self) -> Vec<String> {
        self.backends.iter().map(|r| (*r.key()).clone()).collect()
    }

    pub async fn set_health(&self, addr: &str, is_healthy: bool) {
        if let Some(mut b) = self.backends.get_mut(addr) {
            b.is_healthy = is_healthy;
        }
    }

    pub async fn inc_backend_connection(&self, addr: &str) {
        if let Some(mut b) = self.backends.get_mut(addr) {
            b.active_connections += 1;
            gauge!("lb_backend_active_connections", "backend" => addr.to_string())
                .set(b.active_connections as f64);
            gauge!("lb_active_connections").increment(1);
        }
    }

    pub async fn dec_backend_connection(&self, addr: &str) {
        if let Some(mut b) = self.backends.get_mut(addr) {
            b.active_connections -= 1;
            gauge!("lb_backend_active_connections", "backend" => addr.to_string())
                .set(b.active_connections as f64);
            gauge!("lb_active_connections").decrement(1);
        }
    }

    pub fn spawn_client_cleanup(&self) {
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now = ClientState::now_ms();
                // 5min ttl
                let expiration = 1000 * 60 * 5;
                clients.retain(|_, state| {
                    let last_seen = state.last_seen_ms.load(Ordering::Relaxed);
                    now - last_seen < expiration
                });
            }
        });
    }

    pub fn add_client(&self, ip: IpAddr) -> Arc<ClientState> {
        self.clients
            .entry(ip)
            .or_insert_with(|| unsafe {
                let connection_quota = Quota::per_second(NonZeroU32::new_unchecked(5));
                let bandwidth_quota = Quota::per_second(NonZeroU32::new_unchecked(100 * 1024))
                    .allow_burst(NonZeroU32::new_unchecked(16 * 1024));

                Arc::new(ClientState::new(connection_quota, bandwidth_quota))
            })
            .clone()
    }
}
