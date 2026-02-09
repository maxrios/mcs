use std::env;

#[derive(Clone)]
pub struct Config {
    pub hostname: String,
    pub port: u16,
    pub db_url: String,
    pub redis_url: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
}

impl Config {
    pub fn load() -> Self {
        let _ = dotenvy::dotenv();

        let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string());
        let port = env::var("MCS_PORT")
            .unwrap_or_else(|_| "64400".to_string())
            .parse()
            .unwrap_or(64400);
        let db_url = env::var("POSTGRES_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/postgres".to_string());
        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let tls_cert_path = env::var("TLS_CERT").unwrap_or_else(|_| "tls/server.cert".to_string());
        let tls_key_path = env::var("TLS_KEY").unwrap_or_else(|_| "tls/server.key".to_string());

        Self {
            hostname,
            port,
            db_url,
            redis_url,
            tls_cert_path,
            tls_key_path,
        }
    }
}
