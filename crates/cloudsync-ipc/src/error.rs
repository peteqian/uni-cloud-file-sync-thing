//! IPC error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Message too large: {size} bytes exceeds {limit} byte limit")]
    MessageTooLarge { size: usize, limit: usize },

    #[error("Handler error: {0}")]
    Handler(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_io() {
        let err = Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "socket not found",
        ));
        assert!(err.to_string().contains("socket not found"));
    }

    #[test]
    fn error_display_json() {
        let json_err = serde_json::from_str::<String>("not valid json").unwrap_err();
        let err = Error::Json(json_err);
        assert!(err.to_string().starts_with("JSON error:"));
    }

    #[test]
    fn error_display_connection_closed() {
        let err = Error::ConnectionClosed;
        assert_eq!(err.to_string(), "Connection closed");
    }

    #[test]
    fn error_display_message_too_large() {
        let err = Error::MessageTooLarge {
            size: 2_000_000,
            limit: 1_048_576,
        };
        assert!(err.to_string().contains("2000000"));
        assert!(err.to_string().contains("1048576"));
    }

    #[test]
    fn error_display_handler() {
        let err = Error::Handler("something went wrong".to_string());
        assert!(err.to_string().contains("something went wrong"));
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn error_from_json() {
        let json_err = serde_json::from_str::<String>("{}").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Json(_)));
    }
}
