use crate::rate_limiter::RateLimitedStream;
use crate::state::lb::LoadBalancerState;
use anyhow::Context;
use anyhow::Result;
use metrics::counter;
use redis::AsyncCommands;
use rustls::ServerConfig;
use rustls_pemfile::Item;
use rustls_pemfile::certs;
use rustls_pemfile::read_one;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{self, Duration};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;
use tracing::{error, info, warn};

pub struct LoadBalancer {
    state: LoadBalancerState,
    redis_url: String,
    bind_addr: String,
    tls_acceptor: TlsAcceptor,
}

impl LoadBalancer {
    pub fn new(
        bind_addr: String,
        redis_url: String,
        tls_cert_path: String,
        tls_key_path: String,
    ) -> Self {
        let certs = Self::load_certs(&tls_cert_path).expect("failed to load certs");
        let key = Self::load_key(&tls_key_path).expect("failed to load private key");
        let tls_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("bad certificate/key combination");
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

        Self {
            state: LoadBalancerState::new(),
            redis_url,
            bind_addr,
            tls_acceptor,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let state_discovery = self.state.clone();
        let redis_url = self.redis_url.clone();
        tokio::spawn(async move {
            Self::discovery_task(state_discovery, redis_url).await;
        });

        let state_health = self.state.clone();
        tokio::spawn(async move {
            Self::health_check_task(state_health).await;
        });

        let listener = TcpListener::bind(&self.bind_addr).await?;
        info!("lb listening on {}", self.bind_addr);
        self.state.spawn_client_cleanup();

        loop {
            let (client_socket, client_addr) = listener.accept().await?;
            let ip = client_addr.ip();
            let lb_state = self.state.clone();
            let client_state = lb_state.add_client(ip);

            client_state.update_seen();
            if client_state.connection_limiter.check().is_err() {
                warn!(%ip, "connection rate limit exceeded");
                continue;
            }

            let acceptor = self.tls_acceptor.clone();

            tokio::spawn(async move {
                match acceptor.accept(client_socket).await {
                    Ok(tls_stream) => {
                        let limited_client_socket = RateLimitedStream::new(
                            tls_stream,
                            client_state.bandwidth_limiter.clone(),
                        );

                        if let Err(e) =
                            Self::handle_connection(lb_state, limited_client_socket).await
                        {
                            warn!(%client_addr, err=?e, "failed to establish connection")
                        }
                    }
                    Err(e) => warn!(%client_addr, err=?e, "TLS handshake failed"),
                }
            });
        }
    }

    async fn handle_connection(
        state: LoadBalancerState,
        mut limited_client_socket: RateLimitedStream<TlsStream<TcpStream>>,
    ) -> Result<()> {
        counter!("lb_total_connections").increment(1);

        let backend_addr = match state.next_backend().await {
            Some(addr) => addr,
            None => {
                return Ok(());
            }
        };

        let mut server_socket = TcpStream::connect(&backend_addr).await?;
        state.inc_backend_connection(&backend_addr).await;

        let result =
            tokio::io::copy_bidirectional(&mut limited_client_socket, &mut server_socket).await;
        state.dec_backend_connection(&backend_addr).await;

        let _ = result?;
        Ok(())
    }

    async fn discovery_task(state: LoadBalancerState, redis_url: String) {
        let client = match redis::Client::open(redis_url) {
            Ok(c) => c,
            Err(e) => {
                error!(err=?e, "failed to create redis client");
                return;
            }
        };

        let mut interval = time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let mut conn = match client.get_multiplexed_async_connection().await {
                Ok(c) => c,
                Err(e) => {
                    warn!(err=?e, "redis connection failed, retrying in 5s");
                    continue;
                }
            };

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let min_score = now.saturating_sub(5);

            let redis_backends: Vec<String> =
                match conn.zrangebyscore("mcs:node", min_score, "+inf").await {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(err=?e, "failed to fetch servers from redis");
                        continue;
                    }
                };

            let current_backends = state.get_backend_addrs().await;
            for addr in &redis_backends {
                if !current_backends.contains(addr) {
                    info!(%addr, "adding backend to registry");
                    state.add_backend(addr.clone(), 0).await;
                }
            }

            for addr in &current_backends {
                if !redis_backends.contains(addr) {
                    warn!(%addr, "removing backend from registery");
                    state.remove_backend(addr).await;
                }
            }
        }
    }

    async fn health_check_task(state: LoadBalancerState) {
        let mut interval = time::interval(Duration::from_secs(3));

        loop {
            interval.tick().await;
            let backend_addrs = state.get_backend_addrs().await;
            for addr in backend_addrs {
                let timeout_duration = Duration::from_millis(500);
                let connect_result =
                    time::timeout(timeout_duration, TcpStream::connect(&addr)).await;

                let is_healthy = match connect_result {
                    Ok(Ok(_)) => true,
                    Ok(Err(_)) | Err(_) => false,
                };
                state.set_health(&addr, is_healthy).await;
                if !is_healthy {
                    warn!(%addr, "backend failed health check");
                    counter!("lb_backend_health_check_failures", "backend" => addr.clone())
                        .increment(1);
                }
            }
        }
    }

    fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
        let file = File::open(path).context(format!("failed to open {}", path))?;
        let mut reader = BufReader::new(file);
        let certs = certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
        Ok(certs)
    }

    fn load_key(path: &str) -> Result<PrivateKeyDer<'static>> {
        let file = File::open(path).context(format!("failed to open {}", path))?;
        let mut reader = BufReader::new(file);

        loop {
            match read_one(&mut reader)? {
                Some(Item::Pkcs1Key(key)) => return Ok(key.into()),
                Some(Item::Pkcs8Key(key)) => return Ok(key.into()),
                Some(Item::Sec1Key(key)) => return Ok(key.into()),
                None => break,
                _ => {}
            }
        }

        Err(anyhow::anyhow!("no valid private key found in {}", path))
    }
}
