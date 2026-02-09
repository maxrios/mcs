use super::{MessageRepository, UserRepository};
use crate::error::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use protocol::ChatPacket;
use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Clone)]
pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl UserRepository for PostgresRepository {
    async fn create_user(&self, username: &str, password: &str) -> Result<()> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)?
            .to_string();

        sqlx::query!(
            "INSERT INTO users (username, password_hash) VALUES ($1, $2) ON CONFLICT (username) DO NOTHING",
            username,
            password_hash).execute(&self.pool).await?;

        Ok(())
    }

    async fn verify_credentials(&self, username: &str, password: &str) -> Result<bool> {
        let row = sqlx::query!(
            "SELECT password_hash FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(record) = row {
            let parsed_hash = PasswordHash::new(&record.password_hash)?;
            return Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok());
        }

        Ok(false)
    }
}

#[async_trait]
impl MessageRepository for PostgresRepository {
    async fn save_message(&self, msg: &ChatPacket) -> Result<()> {
        sqlx::query!(
            "INSERT INTO messages (sender, content, timestamp) VALUES ($1, $2, $3)",
            msg.sender,
            msg.content,
            msg.timestamp
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_recent_messages(&self, before_ts: i64) -> Result<Vec<ChatPacket>> {
        let rows = sqlx::query!(
            "SELECT sender, content, timestamp FROM messages
            WHERE timestamp < $1::BIGINT
            ORDER BY timestamp DESC LIMIT 50",
            before_ts
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ChatPacket {
                sender: r.sender,
                content: r.content,
                timestamp: r.timestamp,
            })
            .rev()
            .collect())
    }
}
