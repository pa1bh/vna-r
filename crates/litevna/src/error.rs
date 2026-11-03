use std::fmt;

/// Error type for everything in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timeout while reading from device: expected {expected} bytes, got {got}")]
    Timeout { expected: usize, got: usize },
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn protocol(msg: impl fmt::Display) -> Self {
        Error::Protocol(msg.to_string())
    }
}
