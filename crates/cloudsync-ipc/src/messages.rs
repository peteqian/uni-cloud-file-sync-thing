//! IPC message types.

use cloudsync_core::FileState;
use serde::{Deserialize, Serialize};

/// Request types sent from shell extensions to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Get sync status for a single file.
    GetStatus { path: String },

    /// Get sync status for multiple files.
    GetStatusBatch { paths: Vec<String> },

    /// Pin or unpin a file for offline access.
    SetPinned { path: String, pinned: bool },

    /// Remove local copy, keep cloud placeholder.
    SetCloudOnly { path: String },

    /// Trigger immediate sync for a file.
    ForceSync { path: String },

    /// Pause sync (globally or per-provider).
    PauseSync { provider: Option<String> },

    /// Resume sync.
    ResumeSync { provider: Option<String> },
}

/// Response types sent from daemon to shell extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Status for a single file.
    Status {
        path: String,
        state: FileState,
        provider: Option<String>,
        error: Option<String>,
    },

    /// Status for multiple files.
    StatusBatch { statuses: Vec<FileStatus> },

    /// Operation completed successfully.
    Ok,

    /// Error occurred.
    Error { message: String },
}

/// Status information for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    pub path: String,
    pub state: FileState,
    pub provider: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_type_tag() {
        let req = Request::GetStatus {
            path: "/home/user/file.txt".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"get_status\""));
    }

    #[test]
    fn response_serializes_with_type_tag() {
        let resp = Response::Status {
            path: "/home/user/file.txt".to_string(),
            state: FileState::Synced,
            provider: Some("gdrive".to_string()),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"status\""));
    }

    #[test]
    fn request_deserializes_from_json() {
        let json = r#"{"type":"get_status","path":"/test/file.txt"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        match req {
            Request::GetStatus { path } => assert_eq!(path, "/test/file.txt"),
            _ => panic!("Wrong request type"),
        }
    }
}
