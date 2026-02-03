use std::{
    error::Error,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use tokio::net::{TcpListener, TcpStream};

struct LoadBalancerState {
    backends: RwLock<Vec<String>>,
    current_index: AtomicUsize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://redis:6379".to_string());
    let redis_client = redis::Client::open(redis_url)?;

    let state = Arc::new(LoadBalancerState {
        backends: RwLock::new(Vec::new()),
        current_index: AtomicUsize::new(0),
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        spawn_discovery(redis_client, state_clone).await;
    });

    let listener = TcpListener::bind("0.0.0.0:64400").await?;
    println!("listening on 0.0.0.0:64400");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("new client: {}", addr);

        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, state).await {
                eprintln!("failed to handle connection: {}", e);
            }
        });
    }
}

async fn spawn_discovery(client: redis::Client, state: Arc<LoadBalancerState>) {
    tokio::spawn(async move {
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;
            let keys = redis::cmd("KEYS")
                .arg("mcs:node:*")
                .query_async::<Vec<String>>(&mut conn)
                .await
                .unwrap();
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
) -> Result<(), Box<dyn Error>> {
    let be_addr = {
        let backends = state.backends.read().unwrap();
        if backends.is_empty() {
            eprintln!("No backends available!");
            return Ok(());
        }
        let index = state.current_index.fetch_add(1, Ordering::Relaxed) % backends.len();
        backends[index].clone()
    };
    println!("forwarding traffic to {}", be_addr);

    let mut be_socket = TcpStream::connect(be_addr).await?;
    let _ = tokio::io::copy_bidirectional(&mut socket, &mut be_socket).await?;
    println!("connection closed.");

    Ok(())
}
