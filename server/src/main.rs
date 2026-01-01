use std::sync::Arc;

use futures::StreamExt;
use protocol::{McsCodec, Message};
use tokio_util::codec::{FramedRead, FramedWrite};

use tokio::net::{TcpListener, tcp::OwnedReadHalf};

mod state;
use state::ChatServer;

#[tokio::main]
async fn main() {
    let host = "127.0.0.1:64400";
    let server = Arc::new(ChatServer::new(host));
    server.clone().spawn_reaper();

    let listener = TcpListener::bind(host).await.unwrap();

    println!("Server running on {}", host);

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let server_ref = Arc::clone(&server);

        tokio::spawn(async move {
            let (reader, writer) = socket.into_split();

            let framed_writer = FramedWrite::new(writer, McsCodec);
            let mut framed_reader = FramedRead::new(reader, McsCodec);

            if let Some(Ok(Message::Join(name))) = framed_reader.next().await {
                if server_ref.register_user(&name, framed_writer).await.is_ok() {
                    server_ref
                        .broadcast("server", format!("{} joined.\n", name), None)
                        .await;

                    handle_session(&name, framed_reader, server_ref).await;
                }
            }
        });
    }
}

async fn handle_session(
    name: &str,
    mut reader: FramedRead<OwnedReadHalf, McsCodec>,
    server: Arc<ChatServer>,
) {
    while let Some(Ok(msg)) = reader.next().await {
        match msg {
            Message::Chat(text) => server.broadcast(&name, format!("{}\n", text), None).await,
            Message::Heartbeat => server.heartbeat(&name).await,
            _ => {}
        }
    }
    server.remove_user(&name).await;
    server
        .broadcast("server", format!("{} left.\n", name), None)
        .await;
}
