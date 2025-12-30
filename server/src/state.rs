use std::{
    collections::HashMap,
    io::{self, Write},
    sync::Arc,
    time::Duration,
};

use futures::SinkExt;
use protocol::{McsCodec, Message};
use tokio::{
    net::tcp::OwnedWriteHalf,
    sync::RwLock,
    time::{Instant, interval},
};
use tokio_util::codec::FramedWrite;

pub type MessageWriter = FramedWrite<OwnedWriteHalf, McsCodec>;
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

    pub async fn broadcast(&self, sender: &str, msg: String, excluded_users: Option<Vec<&String>>) {
        let formatted = if sender == "server" {
            msg.into()
        } else {
            format!("{}: {}", sender, msg)
        };

        self.chat_history.write().await.push(formatted.clone());
        print!("{}", formatted);
        io::stdout().flush().unwrap();

        let mut users = self.active_users.write().await;
        let some_excluded_users = excluded_users.unwrap_or_else(|| Vec::new());
        for (name, user) in users.iter_mut() {
            if name == sender || some_excluded_users.contains(&name) {
                continue;
            }
            // TODO: Make this async
            let _ = user.writer.send(Message::Chat(formatted.clone())).await;
        }
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

        let history = self.chat_history.read().await.join("");
        let _ = writer.send(Message::Chat(history.clone())).await;

        users.insert(
            name.into(),
            ConnectedUser {
                writer,
                last_seen: Instant::now(),
            },
        );

        Ok(())
    }

    pub async fn remove_user(&self, name: &str) {
        let mut users = self.active_users.write().await;
        users.remove(name);
    }

    pub async fn heartbeat(&self, name: &str) {
        let mut users = self.active_users.write().await;
        if let Some(u) = users.get_mut(name) {
            u.last_seen = tokio::time::Instant::now();
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
                    self.broadcast("server", format!("{} timed out.", name), None)
                        .await;
                }
            }
        });
    }
}
