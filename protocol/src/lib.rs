use std::io::{Error, ErrorKind::InvalidData};

use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

pub struct McsCodec;

#[derive(Debug, Clone)]
pub enum Message {
    Chat(String),
    Join(String),
    Heartbeat,
    Error(String),
}

impl Decoder for McsCodec {
    type Item = Message;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Confirms header size: first byte is message type, next 4 bytes is payload size
        if src.len() < 5 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[1..5]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        // Checks if full payload has arrived
        if src.len() < 5 + length {
            return Ok(None);
        }

        let msg_type = src.get_u8();
        src.advance(4);
        let payload = src.split_to(length);

        match msg_type {
            1 => {
                let s = String::from_utf8(payload.to_vec()).map_err(|_| InvalidData)?;
                Ok(Option::from(Message::Chat(s)))
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
            Message::Chat(text) => {
                let payload = text.as_bytes();
                dst.put_u8(0x01);
                dst.put_u32(payload.len() as u32);
                dst.extend_from_slice(payload);
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
    use super::McsCodec;
    use super::Message;
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn encode_decode_cycle_succeeds() {
        let mut buf = BytesMut::new();
        let original_msg_text = "Some Message";
        let original_msg = Message::Chat(original_msg_text.to_string());

        McsCodec.encode(original_msg.clone(), &mut buf).unwrap();
        let decode_msg = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("Should return a message");

        if let Message::Chat(msg) = decode_msg {
            assert_eq!(msg, original_msg_text);
        } else {
            panic!("Decoded wrong message type");
        }
    }

    #[test]
    fn partial_packet_decoding_succeeds() {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[0x01, 0x00, 0x00]);

        let result = McsCodec.decode(&mut buf).unwrap();
        assert!(result.is_none());
        assert_eq!(buf.len(), 3);

        buf.extend_from_slice(&[0x00, 0x05]);
        buf.extend_from_slice(b"Hello");

        let result = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("Should now have a full message");
        if let Message::Chat(msg) = result {
            assert_eq!(msg, "Hello");
        }
    }

    #[test]
    fn multiple_messages_in_buffer_succeeds() {
        let mut buf = BytesMut::new();

        McsCodec
            .encode(Message::Chat("Message 1".to_string()), &mut buf)
            .unwrap();
        McsCodec
            .encode(Message::Chat("Message 2".to_string()), &mut buf)
            .unwrap();

        let _ = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("Should get message 1");
        let _ = McsCodec
            .decode(&mut buf)
            .unwrap()
            .expect("Should get message 2");

        assert!(buf.is_empty());
    }
}
