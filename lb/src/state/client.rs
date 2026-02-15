use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};

pub struct ClientState {
    pub connection_limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    pub bandwidth_limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    pub last_seen_ms: AtomicU64,
}

impl ClientState {
    pub fn new(connection_quota: Quota, bandwidth_quota: Quota) -> Self {
        Self {
            connection_limiter: RateLimiter::direct(connection_quota),
            bandwidth_limiter: Arc::new(RateLimiter::direct(bandwidth_quota)),
            last_seen_ms: AtomicU64::new(Self::now_ms()),
        }
    }

    pub fn update_seen(&self) {
        self.last_seen_ms.store(Self::now_ms(), Ordering::Relaxed);
    }

    pub fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}
