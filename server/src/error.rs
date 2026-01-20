use protocol::Message;
use thiserror::Error;
use tokio::sync::broadcast::error::SendError;

#[derive(Error, Debug)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("network error: {0}")]
    Network(#[from] SendError<Message>),

    #[error("username '{0}' is already taken")]
    UsernameTaken(String),

    #[error("username '{0}' is too short")]
    UsernameTooShort(String),
}
