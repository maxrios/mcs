use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use protocol::ChatPacket;
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::error::Result;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        sqlx::migrate!().run(&pool).await.unwrap();

        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS messages (
                id SERIAL PRIMARY KEY,
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp BIGINT NOT NULL
            );
            ",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS users (
                username TEXT PRIMARY KEY,
                password_hash TEXT NOT NULL
            );
            ",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn verify_credentials(&self, username: &str, password: &str) -> Result<bool> {
        let row = sqlx::query!(
            "SELECT password_hash FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let hash_str = row.password_hash;
            let parsed_hash = PasswordHash::new(&hash_str)?;

            return Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok());
        }

        Ok(false)
    }

    pub async fn create_user(&self, username: &str, password: &str) -> Result<()> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)?
            .to_string();

        sqlx::query!(
            r"INSERT INTO users (username, password_hash)
              VALUES ($1, $2) ON CONFLICT (username) DO NOTHING",
            username,
            &password_hash
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_message(&self, msg: &ChatPacket) -> Result<()> {
        sqlx::query!(
            r"INSERT INTO messages (sender, content, timestamp)
              VALUES ($1, $2, $3)",
            &msg.sender,
            &msg.content,
            msg.timestamp,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_recent_messages(&self, timestamp: i64) -> Result<Vec<ChatPacket>> {
        let rows = sqlx::query!(
            r"SELECT sender, content, timestamp
              FROM messages WHERE timestamp < $1
              ORDER BY timestamp DESC LIMIT 50",
            timestamp
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ChatPacket {
                sender: row.sender,
                content: row.content,
                timestamp: row.timestamp,
            })
            .rev()
            .collect())
    }
}
