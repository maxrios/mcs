use std::{
    collections::HashMap,
    io::{self, Write},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures::{Sink, SinkExt, future::join_all};
use protocol::Message;
use tokio::{
    sync::RwLock,
    time::{Instant, interval},
};

pub type MessageWriter = Pin<Box<dyn Sink<Message, Error = std::io::Error> + Send + Sync + Unpin>>;
type UserMap = Arc<RwLock<HashMap<String, ConnectedUser>>>;
type ChatVec = Arc<RwLock<Vec<String>>>;

pub struct ConnectedUser {
    pub writer: MessageWriter,
    pub last_seen: Instant,
}

pub struct ChatServer {
    host: String,
    active_users: UserMap,
    chat_history: ChatVec,
}

impl ChatServer {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
            active_users: Arc::new(RwLock::new(HashMap::new())),
            chat_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn broadcast(&self, sender: &str, msg: String) {
        let formatted = if sender == "server" {
            msg.into()
        } else {
            format!("{}: {}", sender, msg)
        };

        self.chat_history.write().await.push(formatted.clone());
        print!("{}", formatted);
        io::stdout().flush().unwrap();

        let mut users = self.active_users.write().await;

        let broadcast_futures = users
            .iter_mut()
            .map(|(_, user)| user.writer.send(Message::Chat(formatted.clone())));

        join_all(broadcast_futures).await;
    }

    pub async fn register_user(&self, name: &str, mut writer: MessageWriter) -> Result<(), String> {
        let mut users = self.active_users.write().await;

        if name.len() < 3 {
            let _ = writer
                .send(Message::Error("Username too short".into()))
                .await;
            return Err("Username too short".into());
        }
        if users.contains_key(name) {
            let _ = writer.send(Message::Error("Username taken".into())).await;
            return Err("Username taken".into());
        }

        let _ = writer
            .send(Message::Chat(format!("Connected to {}", self.host)))
            .await;

        let history = self.chat_history.read().await;
        for msg in history.iter() {
            let _ = writer.send(Message::Chat(msg.clone())).await;
        }

        users.insert(
            name.into(),
            ConnectedUser {
                writer,
                last_seen: Instant::now(),
            },
        );

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) -> Option<ConnectedUser> {
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
                    self.broadcast("server", format!("{} timed out.", name))
                        .await;
                }
            }
        });
    }
}

#[cfg(test)]
mod test {
    use std::io::{Error, ErrorKind};

    use futures::{SinkExt, StreamExt, channel::mpsc};
    use protocol::Message;

    use crate::state::{ChatServer, MessageWriter};

    fn create_mock_client() -> (MessageWriter, mpsc::UnboundedReceiver<Message>) {
        let (tx, rx) = mpsc::unbounded::<Message>();

        let boxed_sink = Box::pin(tx.sink_map_err(|e| Error::new(ErrorKind::Other, e.to_string())));

        (boxed_sink, rx)
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

        server.broadcast("user_1", "test".to_string()).await;

        match rx_user_2.next().await {
            Some(Message::Chat(msg)) => assert_eq!(msg, "user_1: test"),
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
}
