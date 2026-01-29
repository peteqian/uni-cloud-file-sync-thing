//! Error types for CloudSync operations.

use thiserror::Error;

/// The main error type for CloudSync operations.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Token expired for provider {provider}")]
    TokenExpired { provider: String },

    #[error("Provider API error: {message}")]
    ProviderApi { provider: String, message: String },

    #[error("Rate limited by {provider}, retry after {retry_after_secs} seconds")]
    RateLimited {
        provider: String,
        retry_after_secs: u64,
    },

    #[error("Network error: {0}")]
    Network(String),

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Conflict detected for file: {path}")]
    Conflict { path: String },

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Provider not supported: {0}")]
    UnsupportedProvider(String),

    #[error("Quota exceeded for provider {provider}")]
    QuotaExceeded { provider: String },

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Cloud-native file cannot be downloaded (open in browser): {url}")]
    CloudNativeFile { url: String },
}

/// A specialized Result type for CloudSync operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_authentication() {
        let err = Error::Authentication("Invalid credentials".to_string());
        assert_eq!(
            err.to_string(),
            "Authentication failed: Invalid credentials"
        );
    }

    #[test]
    fn error_display_token_expired() {
        let err = Error::TokenExpired {
            provider: "gdrive".to_string(),
        };
        assert_eq!(err.to_string(), "Token expired for provider gdrive");
    }

    #[test]
    fn error_display_rate_limited() {
        let err = Error::RateLimited {
            provider: "dropbox".to_string(),
            retry_after_secs: 60,
        };
        assert_eq!(
            err.to_string(),
            "Rate limited by dropbox, retry after 60 seconds"
        );
    }

    #[test]
    fn error_display_conflict() {
        let err = Error::Conflict {
            path: "/documents/report.pdf".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Conflict detected for file: /documents/report.pdf"
        );
    }

    #[test]
    fn error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn result_type_works() {
        fn returns_ok() -> Result<i32> {
            Ok(42)
        }

        fn returns_err() -> Result<i32> {
            Err(Error::Cancelled)
        }

        assert_eq!(returns_ok().unwrap(), 42);
        assert!(returns_err().is_err());
    }
}
