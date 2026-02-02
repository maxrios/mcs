#![warn(clippy::all, clippy::pedantic, clippy::nursery, unused_extern_crates)]

use std::io::Error;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

pub struct McsCodec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPacket {
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Chat(ChatPacket),
    Join(String),
    Heartbeat,
    Error(ChatError),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Error)]
pub enum ChatError {
    #[error("network error")]
    Network,

    #[error("username already taken")]
    UsernameTaken,

    #[error("username too short")]
    UsernameTooShort,

    #[error("internal error")]
    Internal,
}

impl Decoder for McsCodec {
    type Item = Message;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[0..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        src.advance(4);
        let payload = src.split_to(length);

        let message = postcard::from_bytes(&payload)
            .map_err(|_| Error::new(std::io::ErrorKind::InvalidData, "deserialzation failed"))?;

        Ok(Some(message))
    }
}

impl Encoder<Message> for McsCodec {
    type Error = Error;

    #[allow(clippy::cast_possible_truncation)]
    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let payload = postcard::to_stdvec(&item)
            .map_err(|_| Error::new(std::io::ErrorKind::InvalidData, "serialization failed"))?;
        dst.put_u32(payload.len() as u32);
        dst.extend_from_slice(&payload);

        Ok(())
    }
}

impl ChatPacket {
    #[must_use]
    pub fn new_server_packet(content: String) -> Self {
        Self {
            sender: "server".to_string(),
            content,
            timestamp: Utc::now().timestamp(),
        }
    }

    #[must_use]
    pub fn new_user_packet(sender: String, content: String) -> Self {
        Self {
            sender,
            content,
            timestamp: Utc::now().timestamp(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ChatError;
    use crate::ChatPacket;

    use super::McsCodec;
    use super::Message;
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn encode_decode_chat_succeeds() {
        let mut buf = BytesMut::new();
        let sender = "sender".to_string();
        let content = "Some Message".to_string();
        let original_msg =
            Message::Chat(ChatPacket::new_user_packet(sender.clone(), content.clone()));

        McsCodec.encode(original_msg, &mut buf).unwrap();
        let decode_msg = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("should return a message");

        if let Message::Chat(msg) = decode_msg {
            assert_eq!(msg.sender, sender);
            assert_eq!(msg.content, content);
        } else {
            panic!("decoded wrong message type");
        }
    }

    #[test]
    fn encode_decode_error_succeeds() {
        let mut buf = BytesMut::new();
        let original_error = Message::Error(ChatError::UsernameTaken);

        McsCodec.encode(original_error, &mut buf).unwrap();
        let decode_msg = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("should return an error");

        if let Message::Error(err) = decode_msg {
            assert_eq!(err, ChatError::UsernameTaken);
        } else {
            panic!("decoded wrong error type");
        }
    }

    #[test]
    fn partial_packet_decoding_succeeds() {
        let mut buf = BytesMut::new();

        let msg1 = Message::Chat(ChatPacket {
            sender: "Alice".to_string(),
            content: "Part 1".to_string(),
            timestamp: 100,
        });

        let msg2 = Message::Chat(ChatPacket {
            sender: "Bob".to_string(),
            content: "Part 2".to_string(),
            timestamp: 200,
        });

        let mut full_stream = BytesMut::new();
        McsCodec.encode(msg1, &mut full_stream).unwrap();
        McsCodec.encode(msg2, &mut full_stream).unwrap();

        let split_point = 10;
        buf.extend_from_slice(&full_stream[..split_point]);

        {
            let result = McsCodec.decode(&mut buf).unwrap();
            assert!(
                result.is_none(),
                "Should return None when data is incomplete"
            );
        }

        buf.extend_from_slice(&full_stream[split_point..]);

        {
            let result = McsCodec
                .decode(&mut buf)
                .unwrap()
                .expect("Should decode message 1");
            if let Message::Chat(msg) = result {
                assert_eq!(msg.sender, "Alice".to_string());
                assert_eq!(msg.content, "Part 1".to_string());
                assert_eq!(msg.timestamp, 100);
            } else {
                panic!("Incorrect Message type")
            }
        }

        {
            let result = McsCodec
                .decode(&mut buf)
                .unwrap()
                .expect("Should decode message 2");
            if let Message::Chat(msg) = result {
                assert_eq!(msg.sender, "Bob".to_string());
                assert_eq!(msg.content, "Part 2".to_string());
                assert_eq!(msg.timestamp, 200);
            } else {
                panic!("Incorrect Message type")
            }
        }

        assert!(buf.is_empty());
    }
}
