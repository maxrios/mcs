use crate::Message;
use thiserror::Error;
use tokio::sync::broadcast::error::SendError;

#[derive(Error, Debug)]
pub enum Error {
    #[cfg(feature = "server")]
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("issue accessing file: {0}")]
    IO(#[from] std::io::Error),

    #[error("network error: {0}")]
    Network(#[from] SendError<Message>),

    #[error("username '{0}' is already taken")]
    UsernameTaken(String),

    #[error("username '{0}' is too short")]
    UsernameTooShort(String),
}
