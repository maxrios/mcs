use std::{collections::HashMap, sync::Arc};

use futures::{SinkExt, StreamExt};
use protocol::{McsCodec, Message};
use tokio_util::codec::{FramedRead, FramedWrite};

use tokio::{
    net::{
        TcpListener,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::RwLock,
};

type MessageWriter = FramedWrite<OwnedWriteHalf, McsCodec>;

#[tokio::main]
async fn main() {
    const HOST: &str = "127.0.0.1:64400";
    let listener = TcpListener::bind(HOST).await.unwrap();
    let chat_history = Arc::new(RwLock::new(Vec::<String>::new()));
    let active_users = Arc::new(RwLock::new(HashMap::<String, MessageWriter>::new()));

    println!("Chat server started...");

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let (reader, writer) = socket.into_split();

        let mut framed_writer = FramedWrite::new(writer, McsCodec);
        let mut framed_reader = FramedRead::new(reader, McsCodec);

        let chat_clone = Arc::clone(&chat_history);
        let users_clone = Arc::clone(&active_users);

        let user = match framed_reader.next().await {
            Some(Ok(Message::Join(username))) => {
                if username.len() < 3 {
                    let _ = framed_writer
                        .send(Message::Error("username is too short".into()))
                        .await;
                    continue;
                } else if active_users.read().await.contains_key(&username) {
                    let _ = framed_writer
                        .send(Message::Error("username already exists".into()))
                        .await;
                    continue;
                }
                let _ = framed_writer
                    .send(Message::Chat(format!("connected to {}", HOST)))
                    .await;
                username
            }
            _ => continue,
        };

        send_history(&mut framed_writer, &chat_history).await;

        update_chat(
            &user,
            &active_users,
            &chat_history,
            format!("{} connected...\n", user),
        )
        .await;

        {
            let mut lock = active_users.write().await;
            lock.insert(user.clone(), framed_writer);
        }

        tokio::spawn(async move {
            spawn_chat(user, framed_reader, users_clone, chat_clone).await;
        });
    }
}

async fn spawn_chat(
    user: String,
    mut reader: FramedRead<OwnedReadHalf, McsCodec>,
    active_users: Arc<RwLock<HashMap<String, MessageWriter>>>,
    chat_history: Arc<RwLock<Vec<String>>>,
) {
    while let Some(result) = reader.next().await {
        match result {
            Ok(Message::Chat(msg)) => {
                let updated_msg = format!("{}: {}", user, msg);
                update_chat(&user, &active_users, &chat_history, updated_msg).await;
            }
            Ok(Message::Error(msg)) => println!("{}", msg),
            Err(_) => break,
            _ => {}
        }
    }
    disconnect(&user, &active_users, &chat_history).await;
}

async fn disconnect(
    user: &String,
    active_users: &Arc<RwLock<HashMap<String, MessageWriter>>>,
    chat_history: &Arc<RwLock<Vec<String>>>,
) {
    let msg = String::from(format!("{} disconnected...\n", user));
    update_chat(&user, active_users, &chat_history, msg).await;
}

async fn update_chat(
    current_user: &String,
    active_users: &Arc<RwLock<HashMap<String, MessageWriter>>>,
    chat_history: &Arc<RwLock<Vec<String>>>,
    msg: String,
) {
    print!("{}", msg);
    chat_history.write().await.push(msg.clone());

    let mut users = active_users.write().await;

    for (user, writer) in users.iter_mut() {
        if user == current_user {
            continue;
        }
        let _ = writer.send(Message::Chat(msg.clone())).await;
    }
}

async fn send_history(writer: &mut MessageWriter, chat_history: &Arc<RwLock<Vec<String>>>) {
    let history = chat_history.read().await;

    for msg in history.iter() {
        let formatted_msg = format!("{}", msg);
        if let Err(e) = writer.send(Message::Chat(formatted_msg)).await {
            println!("Failed to send history: {}", e);
            break;
        }
    }
}
