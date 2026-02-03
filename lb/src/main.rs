use std::{
    error::Error,
    net::SocketAddr,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tracing::{error, info};

use tokio::net::{TcpListener, TcpStream};

struct LoadBalancerState {
    backends: RwLock<Vec<String>>,
    current_index: AtomicUsize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mcs_port = std::env::var("MCS_PORT").unwrap_or_else(|_| "64400".to_string());
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://redis:6379".to_string());
    let redis_client = redis::Client::open(redis_url)?;
    let state = Arc::new(LoadBalancerState {
        backends: RwLock::new(Vec::new()),
        current_index: AtomicUsize::new(0),
    });
    let host = format!("0.0.0.0:{mcs_port}");

    let state_clone = state.clone();
    tokio::spawn(async move {
        spawn_discovery(redis_client, state_clone).await;
    });

    let listener = TcpListener::bind(&host).await?;
    info!(%host, "load balancer running");

    loop {
        let (socket, addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, state, addr).await {
                error!(err = ?e, "failed to handle connection");
            }
        });
    }
}

async fn spawn_discovery(client: redis::Client, state: Arc<LoadBalancerState>) {
    tokio::spawn(async move {
        let mut conn = match client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                error!(err = ?e, "failed to connect to redis");
                return;
            }
        };
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;
            let keys = match redis::cmd("KEYS")
                .arg("mcs:node:*")
                .query_async::<Vec<String>>(&mut conn)
                .await
            {
                Ok(keys) => keys,
                Err(e) => {
                    error!(err = ?e, "failed to retrieve server keys");
                    return;
                }
            };
            let backends = keys
                .into_iter()
                .filter_map(|k| k.strip_prefix("mcs:node:").map(String::from))
                .collect::<Vec<String>>();

            if !backends.is_empty()
                && let Ok(mut w) = state.backends.write()
            {
                *w = backends;
            }
        }
    });
}

async fn handle_connection(
    mut socket: TcpStream,
    state: Arc<LoadBalancerState>,
    addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let be_addr = {
        let backends = state
            .backends
            .read()
            .map_err(|_| std::io::Error::other("backend lock poisoned"))?;
        if backends.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no mcs servers were found",
            )
            .into());
        }
        let index = state.current_index.fetch_add(1, Ordering::Relaxed) % backends.len();
        backends[index].clone()
    };
    info!(client = ?addr, host = ?be_addr, "routing client connection to host");

    let mut be_socket = TcpStream::connect(&be_addr).await?;
    let _ = tokio::io::copy_bidirectional(&mut socket, &mut be_socket).await?;
    info!(client = ?addr, host = ?be_addr, "closing client connection to host");

    Ok(())
}
