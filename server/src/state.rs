use std::{
    io::{self, Write},
    sync::Arc,
    time::Duration,
};

use chrono::{TimeZone, Utc};
use dashmap::DashMap;
use protocol::{ChatPacket, Message};
use tokio::{
    sync::{RwLock, broadcast},
    time::{Instant, interval},
};

type UserMap = Arc<DashMap<String, ConnectedUser>>;
type ChatVec = Arc<RwLock<Vec<ChatPacket>>>;

pub struct ConnectedUser {
    pub last_seen: Instant,
}

pub struct ChatServer {
    channel_tx: broadcast::Sender<Message>,
    active_users: UserMap,
    chat_history: ChatVec,
}

impl ChatServer {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);

        Self {
            channel_tx: tx,
            active_users: Arc::new(DashMap::new()),
            chat_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.channel_tx.subscribe()
    }

    pub async fn get_history(&self) -> Vec<ChatPacket> {
        self.chat_history.read().await.clone()
    }

    pub async fn broadcast(&self, msg: ChatPacket) {
        let datetime = Utc
            .timestamp_opt(msg.timestamp, 0)
            .single()
            .expect("Invalid timestamp");
        if msg.sender == "server" {
            print!("[{}] {}", datetime, msg.content);
        } else {
            println!("[{}] {}: {}", datetime, msg.sender, msg.content);
        }
        io::stdout().flush().unwrap();

        let _ = self.channel_tx.send(Message::Chat(msg.clone()));
        self.chat_history.write().await.push(msg);
    }

    pub async fn register_user(&self, name: &str) -> Result<(), String> {
        if name.len() < 3 {
            return Err("Username too short".into());
        }

        if self.active_users.contains_key(name) || name == "server" || name == "client" {
            return Err("Username taken".into());
        }

        self.active_users.insert(
            name.into(),
            ConnectedUser {
                last_seen: Instant::now(),
            },
        );

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) {
        self.active_users.remove(name);
    }

    pub async fn heartbeat(&self, name: &str) -> bool {
        if let Some(mut u) = self.active_users.get_mut(name) {
            u.last_seen = Instant::now();
            true
        } else {
            false
        }
    }

    pub fn spawn_reaper(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));

            loop {
                interval.tick().await;
                let now = Instant::now();
                let mut timed_out_users = Vec::new();

                {
                    self.active_users.retain(|name, user| {
                        if now.duration_since(user.last_seen).as_secs() > 30 {
                            timed_out_users.push(name.clone());
                            false
                        } else {
                            true
                        }
                    });
                }

                for name in timed_out_users {
                    self.broadcast(ChatPacket::new_server_packet(format!(
                        "{} timed out.",
                        name
                    )))
                    .await;
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

    #[tokio::test]
    async fn broadcast_succeeds() {
        let server = ChatServer::new();
        let mut rx = server.subscribe();

        server
            .broadcast(ChatPacket::new_user_packet(
                "user_1".to_string(),
                "test".to_string(),
            ))
            .await;

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
        let server = ChatServer::new();

        let result = server.register_user("user_1").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn register_user_too_short_fails() {
        let server = ChatServer::new();

        let result = server.register_user("u").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn register_user_taken_fails() {
        let server = ChatServer::new();

        let _ = server.register_user("user_1").await;
        let result = server.register_user("user_1").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_user_succeeds() {
        let server = ChatServer::new();

        let _ = server.register_user("user_1").await;
        server.remove_user("user_1").await;
        let result = server.register_user("user_1").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn heartbeat_succeeds() {
        let server = ChatServer::new();

        let _ = server.register_user("user_1").await;
        let last_seen_first = if let Some(user) = server.active_users.get("user_1") {
            user.last_seen
        } else {
            panic!("user not found")
        };

        assert!(server.heartbeat("user_1").await);

        let last_seen_second = if let Some(user) = server.active_users.get("user_1") {
            user.last_seen
        } else {
            panic!("user not found")
        };

        assert_ne!(last_seen_first, last_seen_second);
    }

    #[tokio::test]
    async fn spawn_reaper_succeeds() {
        pause();

        let server = Arc::new(ChatServer::new());
        let _ = server.register_user("user_1").await;

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
