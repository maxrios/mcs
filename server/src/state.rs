use std::{
    collections::HashMap,
    io::{self, Write},
    sync::Arc,
    time::Duration,
};

use chrono::{TimeZone, Utc};
use futures::{SinkExt, future::join_all};
use protocol::{ChatPacket, McsCodec, Message};
use tokio::{
    io::AsyncWrite,
    sync::RwLock,
    time::{Instant, interval},
};
use tokio_util::codec::FramedWrite;

type UserMap<W> = Arc<RwLock<HashMap<String, ConnectedUser<W>>>>;
type ChatVec = Arc<RwLock<Vec<ChatPacket>>>;

pub struct ConnectedUser<W> {
    pub writer: FramedWrite<W, McsCodec>,
    pub last_seen: Instant,
}

pub struct ChatServer<W> {
    host: String,
    active_users: UserMap<W>,
    chat_history: ChatVec,
}

impl<W: AsyncWrite + Unpin + Send + Sync + 'static> ChatServer<W> {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
            active_users: Arc::new(RwLock::new(HashMap::new())),
            chat_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn broadcast(&self, msg: ChatPacket) {
        self.chat_history.write().await.push(msg.clone());
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

        let mut users = self.active_users.write().await;

        let broadcast_futures = users
            .iter_mut()
            .map(|(_, user)| user.writer.send(Message::Chat(msg.clone())));

        join_all(broadcast_futures).await;
    }

    pub async fn register_user(&self, name: &str, writer: W) -> Result<(), String> {
        let mut framed_writer = FramedWrite::new(writer, McsCodec);
        let mut users = self.active_users.write().await;

        if name.len() < 3 {
            let _ = framed_writer
                .send(Message::Error("Username too short".into()))
                .await;
            return Err("Username too short".into());
        }

        if users.contains_key(name) || name == "server" || name == "client" {
            let _ = framed_writer
                .send(Message::Error("Username taken".into()))
                .await;
            return Err("Username taken".into());
        }

        let _ = framed_writer
            .send(Message::Chat(ChatPacket::new_server_packet(format!(
                "Connected to {}",
                self.host
            ))))
            .await;

        let history = self.chat_history.read().await;
        for msg in history.iter() {
            let _ = framed_writer.send(Message::Chat(msg.clone())).await;
        }

        users.insert(
            name.into(),
            ConnectedUser {
                writer: framed_writer,
                last_seen: Instant::now(),
            },
        );

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) -> Option<ConnectedUser<W>> {
        let mut users = self.active_users.write().await;
        users.remove(name)
    }

    pub async fn heartbeat(&self, name: &str) -> bool {
        let mut users = self.active_users.write().await;
        if let Some(u) = users.get_mut(name) {
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
                    let mut users = self.active_users.write().await;
                    users.retain(|name, user| {
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

    use futures::StreamExt;
    use protocol::{ChatPacket, McsCodec, Message};
    use tokio::{
        io::{DuplexStream, duplex},
        task::yield_now,
        time::{advance, pause},
    };
    use tokio_util::codec::FramedRead;

    use crate::state::ChatServer;

    fn create_mock_client() -> (DuplexStream, FramedRead<DuplexStream, McsCodec>) {
        let (client, server) = duplex(64);

        let client_reader = FramedRead::new(client, McsCodec);

        (server, client_reader)
    }

    #[tokio::test]
    async fn broadcast_succeeds() {
        let server = ChatServer::new("server");

        let (writer_user_1, mut rx_user_1) = create_mock_client();
        let (writer_user_2, mut rx_user_2) = create_mock_client();

        let _ = server.register_user("user_1", writer_user_1).await;
        rx_user_1.next().await;

        let _ = server.register_user("user_2", writer_user_2).await;
        rx_user_2.next().await;

        server
            .broadcast(ChatPacket::new_user_packet(
                "user_1".to_string(),
                "test".to_string(),
            ))
            .await;

        match rx_user_2.next().await {
            Some(Ok(Message::Chat(msg))) => {
                assert_eq!(msg.sender, "user_1");
                assert_eq!(msg.content, "test");
            }
            _ => panic!("user_2 did not receive the expected broadcast"),
        }
    }

    #[tokio::test]
    async fn register_user_succeeds() {
        let server = ChatServer::new("server");
        let (writer, _) = create_mock_client();

        let result = server.register_user("user_1", writer).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn register_user_too_short_fails() {
        let server = ChatServer::new("server");
        let (writer, _) = create_mock_client();

        let result = server.register_user("u", writer).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn register_user_taken_fails() {
        let server = ChatServer::new("server");
        let (writer_user_1, _) = create_mock_client();
        let (writer_user_2, _) = create_mock_client();

        let _ = server.register_user("user_1", writer_user_1).await;
        let result = server.register_user("user_1", writer_user_2).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_user_succeeds() {
        let server = ChatServer::new("server");
        let (writer_user_1, _) = create_mock_client();
        let (writer_user_2, _) = create_mock_client();

        let _ = server.register_user("user_1", writer_user_1).await;
        server.remove_user("user_1").await;
        let result = server.register_user("user_1", writer_user_2).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn heartbeat_succeeds() {
        let server = ChatServer::new("server");
        let (writer_user_1, _) = create_mock_client();

        let _ = server.register_user("user_1", writer_user_1).await;
        let last_seen_first = if let Some(user) = server.active_users.read().await.get("user_1") {
            user.last_seen
        } else {
            panic!("user not found")
        };

        assert!(server.heartbeat("user_1").await);

        let last_seen_second = if let Some(user) = server.active_users.read().await.get("user_1") {
            user.last_seen
        } else {
            panic!("user not found")
        };

        assert_ne!(last_seen_first, last_seen_second);
    }

    #[tokio::test]
    async fn spawn_reaper_succeeds() {
        pause();

        let server = Arc::new(ChatServer::new("server"));
        let (writer, _) = create_mock_client();
        let _ = server.register_user("user_1", writer).await;

        server.clone().spawn_reaper();
        advance(Duration::from_secs(20)).await;
        yield_now().await;

        assert!(server.active_users.read().await.contains_key("user_1"));

        advance(Duration::from_secs(11)).await;
        yield_now().await;

        assert!(
            !server.active_users.read().await.contains_key("user_1"),
            "user shoud have been reaped"
        );
    }
}
