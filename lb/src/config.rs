use std::env;

#[derive(Debug)]
pub struct Config {
    pub host: String,
    pub host_port: u16,
    pub prometheus_port: u16,
    pub redis_url: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
}

impl Config {
    pub fn load() -> Self {
        let _ = dotenvy::dotenv();
        let host = "0.0.0.0".to_string();
        let host_port = env::var("MCS_PORT")
            .unwrap_or_else(|_| "64400".to_string())
            .parse()
            .unwrap_or(64400);
        let prometheus_port = env::var("PROMETHEUS_PORT")
            .unwrap_or_else(|_| "9000".to_string())
            .parse()
            .unwrap_or(9000);
        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let tls_cert_path = env::var("TLS_CERT").unwrap_or_else(|_| "tls/server.cert".to_string());
        let tls_key_path = env::var("TLS_KEY").unwrap_or_else(|_| "tls/server.key".to_string());

        Self {
            host,
            host_port,
            prometheus_port,
            redis_url,
            tls_cert_path,
            tls_key_path,
        }
    }
}
