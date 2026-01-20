use std::{sync::Arc, time::Duration};

use crate::{db::Database, error::Error};

use dashmap::DashMap;
use protocol::{ChatPacket, Message};
use tokio::{
    sync::broadcast,
    time::{Instant, interval},
};

type UserHeartbeatMap = Arc<DashMap<String, Instant>>;

pub struct ChatServer {
    channel_tx: broadcast::Sender<Message>,
    active_users: UserHeartbeatMap,
    db: Database,
}

impl ChatServer {
    pub async fn new(database_url: &str) -> Result<Self, Error> {
        let (tx, _) = broadcast::channel(100);
        let db = match Database::new(database_url).await {
            Ok(db) => db,
            Err(e) => {
                return Err(Error::Database(e));
            }
        };

        Ok(Self {
            channel_tx: tx,
            active_users: Arc::new(DashMap::new()),
            db,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.channel_tx.subscribe()
    }

    pub async fn get_history(&self) -> Vec<ChatPacket> {
        match self.db.get_recent_messages().await {
            Ok(msgs) => msgs,
            Err(e) => {
                eprintln!("Failed to retrieve messages: {}", e);
                Vec::new()
            }
        }
    }

    pub async fn broadcast(&self, msg: ChatPacket) -> Result<(), Error> {
        if let Err(e) = self.db.save_message(&msg).await {
            return Err(Error::Database(e));
        }

        if let Err(e) = self.channel_tx.send(Message::Chat(msg)) {
            return Err(Error::Network(e));
        }

        Ok(())
    }

    pub async fn register_user(&self, name: &str) -> Result<(), Error> {
        if name.len() < 3 {
            return Err(Error::UsernameTooShort(name.to_string()));
        }

        if self.active_users.contains_key(name) || name == "server" || name == "client" {
            return Err(Error::UsernameTaken(name.to_string()));
        }

        self.active_users.insert(name.into(), Instant::now());

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) {
        self.active_users.remove(name);
    }

    pub async fn heartbeat(&self, name: &str) -> bool {
        self.active_users
            .insert(name.to_string(), Instant::now())
            .is_some()
    }

    pub fn spawn_reaper(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));

            loop {
                interval.tick().await;
                let now = Instant::now();
                let mut timed_out_users = Vec::new();

                {
                    self.active_users.retain(|name, &mut last_seen| {
                        if now.duration_since(last_seen).as_secs() > 30 {
                            timed_out_users.push(name.clone());
                            false
                        } else {
                            true
                        }
                    });
                }

                for name in timed_out_users {
                    if let Err(e) = self
                        .broadcast(ChatPacket::new_server_packet(format!(
                            "{} timed out.",
                            name
                        )))
                        .await
                    {
                        eprintln!("{}", e);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use protocol::{ChatPacket, Message};
    use tokio::{
        task::yield_now,
        time::{advance, pause},
    };

    use crate::state::ChatServer;

    async fn setup_test_server() -> Arc<ChatServer> {
        let db_url = "postgres://postgres:password@localhost:5432/postgres";
        Arc::new(
            ChatServer::new(db_url)
                .await
                .expect("Failed to initialize test database"),
        )
    }

    #[tokio::test]
    async fn broadcast_succeeds() {
        let server = setup_test_server().await;
        let mut rx = server.subscribe();

        server
            .broadcast(ChatPacket::new_user_packet(
                "user_1".to_string(),
                "test".to_string(),
            ))
            .await
            .expect("failed to broadcast message");

        match rx.recv().await {
            Ok(Message::Chat(msg)) => {
                assert_eq!(msg.sender, "user_1");
                assert_eq!(msg.content, "test");
            }
            _ => panic!("user_2 did not receive the expected broadcast"),
        }
    }

    #[tokio::test]
    async fn register_user_succeeds() {
        let server = setup_test_server().await;

        let result = server.register_user("user_1").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn register_user_too_short_fails() {
        let server = setup_test_server().await;

        let result = server.register_user("u").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn register_user_taken_fails() {
        let server = setup_test_server().await;

        let _ = server.register_user("user_1").await;
        let result = server.register_user("user_1").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_user_succeeds() {
        let server = setup_test_server().await;

        let _ = server.register_user("user_1").await;
        server.remove_user("user_1").await;
        let result = server.register_user("user_1").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn heartbeat_succeeds() {
        let server = setup_test_server().await;

        let _ = server.register_user("user_1").await;
        let last_seen_first = if let Some(entry) = server.active_users.get("user_1") {
            *entry.value()
        } else {
            panic!("user not found")
        };

        assert!(server.heartbeat("user_1").await);

        let last_seen_second = if let Some(entry) = server.active_users.get("user_1") {
            *entry.value()
        } else {
            panic!("user not found")
        };

        assert_ne!(last_seen_first, last_seen_second);
    }

    #[tokio::test]
    async fn spawn_reaper_succeeds() {
        let server = setup_test_server().await;
        let _ = server.register_user("user_1").await;

        pause();

        server.clone().spawn_reaper();

        advance(Duration::from_secs(20)).await;
        yield_now().await;

        assert!(server.active_users.contains_key("user_1"));

        advance(Duration::from_secs(11)).await;
        yield_now().await;

        assert!(
            !server.active_users.contains_key("user_1"),
            "user shoud have been reaped"
        );
    }
}
