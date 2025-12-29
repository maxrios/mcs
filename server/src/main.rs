use std::{collections::HashMap, net::SocketAddr, sync::Arc};

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
    let listener = TcpListener::bind("127.0.0.1:64400").await.unwrap();
    let chat_history = Arc::new(RwLock::new(Vec::<String>::new()));
    let active_users = Arc::new(RwLock::new(HashMap::<String, MessageWriter>::new()));

    println!("Chat server started...");

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        let user = String::from(format!("{}:{}", addr.ip(), addr.port()));
        let (reader, writer) = socket.into_split();

        let mut framed_writer = FramedWrite::new(writer, McsCodec);
        let framed_reader = FramedRead::new(reader, McsCodec);

        let chat_clone = Arc::clone(&chat_history);
        let users_clone = Arc::clone(&active_users);

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
            spawn_chat(framed_reader, addr, users_clone, chat_clone).await;
        });
    }
}

async fn spawn_chat(
    mut reader: FramedRead<OwnedReadHalf, McsCodec>,
    addr: SocketAddr,
    active_users: Arc<RwLock<HashMap<String, MessageWriter>>>,
    chat_history: Arc<RwLock<Vec<String>>>,
) {
    let user = String::from(format!("{}:{}", addr.ip(), addr.port()));

    while let Some(result) = reader.next().await {
        match result {
            Ok(Message::Chat(msg)) => {
                let updated_msg = format!("{}: {}", user, msg);
                update_chat(&user, &active_users, &chat_history, updated_msg).await;
            }
            Ok(Message::Join(_)) => {}
            Ok(Message::Heartbeat) => {}
            Err(_) => break,
        }
    }
    disconnect(&active_users, addr, &chat_history).await;
}

async fn disconnect(
    active_users: &Arc<RwLock<HashMap<String, MessageWriter>>>,
    addr: SocketAddr,
    chat_history: &Arc<RwLock<Vec<String>>>,
) {
    let user = String::from(format!("{}:{}", addr.ip(), addr.port()));
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
