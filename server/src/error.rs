use protocol::{ChatError, Message};
use thiserror::Error;
use tokio::sync::broadcast::error::SendError;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("issue accessing file: {0}")]
    IO(#[from] std::io::Error),

    #[error("network error: {0}")]
    Network(#[from] SendError<Message>),

    #[error("serialization error: {0}")]
    Serialization(#[from] postcard::Error),

    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("username '{0}' is already taken")]
    UsernameTaken(String),

    #[error("username '{0}' is too short")]
    UsernameTooShort(String),
}

impl Error {
    pub fn to_chat_error(&self) -> ChatError {
        match self {
            Error::Network(_) => ChatError::Network,
            Error::UsernameTaken(_) => ChatError::UsernameTaken,
            Error::UsernameTooShort(_) => ChatError::UsernameTooShort,
            _ => ChatError::Internal,
        }
    }
}
