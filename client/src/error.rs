use thiserror::Error;
use tokio::sync::mpsc;

/// Helper alias for Result
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TLS configuration error: {0}")]
    Tls(String),

    #[error("Certificate error: {0}")]
    Cert(String),

    #[error("Connection failed: {0}")]
    Connect(String),

    #[error("Network channel closed")]
    ChannelClosed,

    #[error("Connection lost")]
    Disconnected,

    #[error("Render error: {0}")]
    Render(String),
}

/// Helper to convert channel send errors.
impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Self::ChannelClosed
    }
}
