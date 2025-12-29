use std::io::Write;

use futures::{SinkExt, StreamExt};
use protocol::{McsCodec, Message};
use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};

#[tokio::main]
async fn main() {
    let stream = match TcpStream::connect("127.0.0.1:64400").await {
        Ok(res) => res,
        Err(..) => {
            println!("couldn't connect to server");
            return;
        }
    };

    println!("connected to server");

    let (reader, writer) = stream.into_split();

    let framed_writer = FramedWrite::new(writer, McsCodec);
    let framed_reader = FramedRead::new(reader, McsCodec);

    tokio::spawn(async move {
        read_messages(framed_reader).await;
    });

    send_messages(framed_writer).await;
}

async fn read_messages(mut reader: FramedRead<OwnedReadHalf, McsCodec>) {
    while let Some(result) = reader.next().await {
        match result {
            Ok(Message::Chat(msg)) => {
                print!("\r\x1b[2K{}Me: ", msg);
                std::io::stdout().flush().unwrap();
            }
            Ok(Message::Join(_)) => {}
            Ok(Message::Heartbeat) => {}
            Err(e) => {
                println!("\nDisconnected from server: {:?}", e);
                break;
            }
        }
    }
}

async fn send_messages(mut writer: FramedWrite<OwnedWriteHalf, McsCodec>) {
    let mut stdin_reader = BufReader::new(io::stdin());
    let mut input_string = String::new();

    loop {
        print!("Me: ");
        std::io::stdout().flush().unwrap();
        input_string.clear();

        if let Err(e) = stdin_reader.read_line(&mut input_string).await {
            println!("failed to read from stdin: {:?}", e);
            break;
        }

        let trimmed = input_string.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "!quit" {
            break;
        }

        if let Err(e) = writer.send(Message::Chat(input_string.to_string())).await {
            println!("Failed to send message: {:?}", e);
            break;
        }
    }
}
