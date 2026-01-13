use std::io::{Error, ErrorKind::InvalidData};

use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

pub struct McsCodec;

#[derive(Debug, Clone)]
pub struct ChatPacket {
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub enum Message {
    Chat(ChatPacket),
    Join(String),
    Heartbeat,
    Error(String),
}

impl Decoder for McsCodec {
    type Item = Message;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 5 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[1..5]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        if src.len() < 5 + length {
            return Ok(None);
        }

        let msg_type = src.get_u8();
        src.advance(4);
        let mut payload = src.split_to(length);

        match msg_type {
            1 => {
                if payload.remaining() < 12 {
                    return Err(Error::new(InvalidData, "Payload too short for ChatPacket"));
                }

                let timestamp = payload.get_i64();
                let name_length = payload.get_u32() as usize;
                let name_bytes = payload.split_to(name_length);
                let sender = String::from_utf8(name_bytes.to_vec()).map_err(|_| InvalidData)?;
                let content = String::from_utf8(payload.to_vec()).map_err(|_| InvalidData)?;

                Ok(Option::from(Message::Chat(ChatPacket {
                    sender,
                    content,
                    timestamp,
                })))
            }
            2 => {
                let s = String::from_utf8(payload.to_vec()).map_err(|_| InvalidData)?;
                Ok(Option::from(Message::Join(s)))
            }
            3 => Ok(Option::from(Message::Heartbeat)),
            4 => {
                let s = String::from_utf8(payload.to_vec()).map_err(|_| InvalidData)?;
                Ok(Option::from(Message::Error(s)))
            }
            _ => Err(Error::new(InvalidData, "Unknown type")),
        }
    }
}

impl Encoder<Message> for McsCodec {
    type Error = Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            Message::Chat(packet) => {
                let sender_bytes = packet.sender.as_bytes();
                let content_bytes = packet.content.as_bytes();

                // timestamp length + sender length + sender bytes + content bytes
                let payload_length = 12 + sender_bytes.len() + content_bytes.len();

                dst.put_u8(0x01);
                dst.put_u32(payload_length as u32);
                dst.put_i64(packet.timestamp);
                dst.put_u32(sender_bytes.len() as u32);
                dst.extend_from_slice(sender_bytes);
                dst.extend_from_slice(content_bytes);
            }
            Message::Join(username) => {
                let payload = username.as_bytes();
                dst.put_u8(0x02);
                dst.put_u32(username.len() as u32);
                dst.extend_from_slice(payload);
            }
            Message::Heartbeat => {
                dst.put_u8(0x03);
                dst.put_u32(0u32);
            }
            Message::Error(text) => {
                let payload = text.as_bytes();
                dst.put_u8(0x04);
                dst.put_u32(text.len() as u32);
                dst.extend_from_slice(payload);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::ChatPacket;

    use super::McsCodec;
    use super::Message;
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn encode_decode_cycle_succeeds() {
        let mut buf = BytesMut::new();
        let sender = "sender".to_string();
        let timestamp = 101;
        let content = "Some Message".to_string();
        let original_msg = Message::Chat(ChatPacket {
            sender: sender.clone(),
            content: content.clone(),
            timestamp,
        });

        McsCodec.encode(original_msg.clone(), &mut buf).unwrap();
        let decode_msg = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("Should return a message");

        if let Message::Chat(msg) = decode_msg {
            assert_eq!(msg.timestamp, timestamp);
            assert_eq!(msg.sender, sender);
            assert_eq!(msg.content, content);
        } else {
            panic!("Decoded wrong message type");
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
