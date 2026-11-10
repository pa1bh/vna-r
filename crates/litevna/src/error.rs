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
    #[error("calibration error: {0}")]
    Calibration(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A file whose contents cannot be read as what it should be (a
    /// Touchstone file). `line` is 1-based, when the problem has one.
    #[error("{}{msg}", line_prefix(.line))]
    Parse { line: Option<usize>, msg: String },
    /// The connected device cannot do this: `what` needs the LiteVNA register
    /// protocol and `device` is something else.
    #[error("{what} needs a LiteVNA (register protocol); this is a {device}")]
    Unsupported { what: String, device: String },
}

fn line_prefix(line: &Option<usize>) -> String {
    line.map(|l| format!("line {l}: ")).unwrap_or_default()
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn protocol(msg: impl fmt::Display) -> Self {
        Error::Protocol(msg.to_string())
    }

    pub(crate) fn parse(line: Option<usize>, msg: String) -> Self {
        Error::Parse { line, msg }
    }

    /// The link to the device itself failed (the port reported an error),
    /// as opposed to the device answering late or wrongly. A pulled cable
    /// usually shows up like this, but can also surface as a [`Error::Timeout`];
    /// a caller that needs to know for sure checks whether the port still exists.
    pub fn is_link_failure(&self) -> bool {
        matches!(self, Error::Io(_) | Error::Serial(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        assert_eq!(
            Error::parse(Some(7), "not a number: \"x\"".into()).to_string(),
            "line 7: not a number: \"x\""
        );
        assert_eq!(Error::parse(None, "no data".into()).to_string(), "no data");
        let e = Error::Unsupported {
            what: "peek".into(),
            device: "NanoVNA".into(),
        };
        assert_eq!(e.to_string(), "peek needs a LiteVNA (register protocol); this is a NanoVNA");
        assert!(Error::Io(std::io::Error::other("gone")).is_link_failure());
        assert!(!Error::Timeout { expected: 4, got: 0 }.is_link_failure());
    }
}
