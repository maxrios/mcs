use crate::{config::Config, core::LoadBalancer};
use anyhow::Result;
use metrics_exporter_prometheus::PrometheusBuilder;
use rustls::crypto::ring;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod core;
mod rate_limiter;
mod state;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lb=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let _ = ring::default_provider().install_default();
    let config = Config::load();
    let builder = PrometheusBuilder::new();
    builder
        .with_http_listener(([0, 0, 0, 0], config.prometheus_port))
        .install()?;
    info!("metrics initialized on port {}", config.prometheus_port);

    let bind_addr = format!("{}:{}", config.host, config.host_port);

    let lb = LoadBalancer::new(
        bind_addr,
        config.redis_url,
        config.tls_cert_path,
        config.tls_key_path,
    );
    let _ = lb.run().await;
    Ok(())
}
