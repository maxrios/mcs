use protocol::ChatPacket;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(database_url: &str) -> Self {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .expect("Failed to connect to the database");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id SERIAL PRIMARY KEY,
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp BIGINT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to initialize database schema");

        Self { pool }
    }

    pub async fn save_message(&self, msg: &ChatPacket) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO messages (sender, content, timestamp) VALUES ($1, $2, $3)")
            .bind(&msg.sender)
            .bind(&msg.content)
            .bind(msg.timestamp)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_recent_messages(&self) -> Result<Vec<ChatPacket>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT sender, content, timestamp FROM messages ORDER BY id DESC LIMIT 50",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ChatPacket {
                sender: row.get("sender"),
                content: row.get("content"),
                timestamp: row.get("timestamp"),
            })
            .rev()
            .collect())
    }
}
