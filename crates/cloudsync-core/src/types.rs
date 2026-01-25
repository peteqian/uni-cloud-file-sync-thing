//! Core types for CloudSync operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a provider account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub String);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Provider-specific file identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub String);

impl FileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A path within a cloud provider's filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CloudPath(String);

impl CloudPath {
    /// Creates a new cloud path from a string.
    /// The path is normalized to use forward slashes and start with '/'.
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let normalized = if path.starts_with('/') {
            path
        } else {
            format!("/{}", path)
        };
        Self(normalized.replace('\\', "/"))
    }

    /// Returns the root path.
    pub fn root() -> Self {
        Self("/".to_string())
    }

    /// Returns the path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the parent path, or None if this is the root.
    pub fn parent(&self) -> Option<CloudPath> {
        if self.0 == "/" {
            return None;
        }
        let trimmed = self.0.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(0) => Some(CloudPath::root()),
            Some(idx) => Some(CloudPath(trimmed[..idx].to_string())),
            None => Some(CloudPath::root()),
        }
    }

    /// Returns the file name component of the path.
    pub fn file_name(&self) -> Option<&str> {
        if self.0 == "/" {
            return None;
        }
        let trimmed = self.0.trim_end_matches('/');
        trimmed.rsplit('/').next()
    }

    /// Joins this path with another component.
    pub fn join(&self, component: &str) -> CloudPath {
        let base = self.0.trim_end_matches('/');
        let component = component.trim_start_matches('/');
        CloudPath(format!("{}/{}", base, component))
    }

    /// Returns whether this path is the root.
    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }
}

impl std::fmt::Display for CloudPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies a cloud provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    GoogleDrive,
    Dropbox,
    OneDrive,
}

impl ProviderId {
    /// Returns the string identifier for the provider.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::GoogleDrive => "gdrive",
            ProviderId::Dropbox => "dropbox",
            ProviderId::OneDrive => "onedrive",
        }
    }

    /// Returns the display name for the provider.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::GoogleDrive => "Google Drive",
            ProviderId::Dropbox => "Dropbox",
            ProviderId::OneDrive => "OneDrive",
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Represents an item (file or folder) in cloud storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudItem {
    /// Provider-specific unique identifier.
    pub id: FileId,

    /// Name of the file or folder.
    pub name: String,

    /// Full path in the cloud filesystem.
    pub path: CloudPath,

    /// Whether this is a folder.
    pub is_folder: bool,

    /// Size in bytes (None for folders).
    pub size: Option<u64>,

    /// Content hash for change detection (provider-specific).
    pub content_hash: Option<String>,

    /// Last modification time.
    pub modified: DateTime<Utc>,

    /// Creation time (if available).
    pub created: Option<DateTime<Utc>>,

    /// MIME type (if available).
    pub mime_type: Option<String>,
}

impl CloudItem {
    /// Creates a new folder item.
    pub fn folder(id: FileId, name: String, path: CloudPath, modified: DateTime<Utc>) -> Self {
        Self {
            id,
            name,
            path,
            is_folder: true,
            size: None,
            content_hash: None,
            modified,
            created: None,
            mime_type: None,
        }
    }

    /// Creates a new file item.
    pub fn file(
        id: FileId,
        name: String,
        path: CloudPath,
        size: u64,
        modified: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            path,
            is_folder: false,
            size: Some(size),
            content_hash: None,
            modified,
            created: None,
            mime_type: None,
        }
    }
}

/// Options for generating share links.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareOptions {
    /// Whether the link allows editing (vs view-only).
    pub allow_edit: bool,

    /// Optional expiration time for the link.
    pub expires_at: Option<DateTime<Utc>>,

    /// Optional password protection.
    pub password: Option<String>,
}

/// Represents a historical version of a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersion {
    /// Version identifier.
    pub id: String,

    /// Modification time of this version.
    pub modified: DateTime<Utc>,

    /// Size of this version in bytes.
    pub size: u64,

    /// Who made this version (if available).
    pub modified_by: Option<String>,
}

/// Progress information for file transfers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransferProgress {
    /// Bytes transferred so far.
    pub bytes_transferred: u64,

    /// Total bytes to transfer.
    pub total_bytes: u64,
}

impl TransferProgress {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_transferred: 0,
            total_bytes,
        }
    }

    /// Returns the progress as a percentage (0.0 to 100.0).
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            return 100.0;
        }
        (self.bytes_transferred as f64 / self.total_bytes as f64) * 100.0
    }

    /// Returns whether the transfer is complete.
    pub fn is_complete(&self) -> bool {
        self.bytes_transferred >= self.total_bytes
    }
}

/// A list of changes from a provider's change feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeList {
    /// The changes in this batch.
    pub changes: Vec<Change>,

    /// Cursor for fetching the next batch.
    pub cursor: String,

    /// Whether there are more changes to fetch.
    pub has_more: bool,
}

/// Represents a single change from a provider's change feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    /// The affected item (None if deleted).
    pub item: Option<CloudItem>,

    /// The file ID that was affected.
    pub file_id: FileId,

    /// Whether the item was deleted.
    pub deleted: bool,

    /// When the change occurred.
    pub timestamp: DateTime<Utc>,
}

/// Channel for sending progress updates during transfers.
pub type ProgressSender = tokio::sync::mpsc::Sender<TransferProgress>;

/// Channel for receiving progress updates during transfers.
pub type ProgressReceiver = tokio::sync::mpsc::Receiver<TransferProgress>;

/// Creates a new progress channel pair.
pub fn progress_channel(buffer: usize) -> (ProgressSender, ProgressReceiver) {
    tokio::sync::mpsc::channel(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_generates_unique_ids() {
        let id1 = AccountId::new();
        let id2 = AccountId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn account_id_from_string() {
        let id = AccountId::from_string("test-account-123");
        assert_eq!(id.0, "test-account-123");
    }

    #[test]
    fn file_id_creation() {
        let id = FileId::new("abc123");
        assert_eq!(id.0, "abc123");
        assert_eq!(format!("{}", id), "abc123");
    }

    #[test]
    fn cloud_path_normalizes_path() {
        let path = CloudPath::new("documents/report.pdf");
        assert_eq!(path.as_str(), "/documents/report.pdf");
    }

    #[test]
    fn cloud_path_handles_backslashes() {
        let path = CloudPath::new("documents\\subfolder\\file.txt");
        assert_eq!(path.as_str(), "/documents/subfolder/file.txt");
    }

    #[test]
    fn cloud_path_root() {
        let root = CloudPath::root();
        assert_eq!(root.as_str(), "/");
        assert!(root.is_root());
    }

    #[test]
    fn cloud_path_parent_of_file() {
        let path = CloudPath::new("/documents/report.pdf");
        let parent = path.parent().unwrap();
        assert_eq!(parent.as_str(), "/documents");
    }

    #[test]
    fn cloud_path_parent_of_root_folder() {
        let path = CloudPath::new("/documents");
        let parent = path.parent().unwrap();
        assert_eq!(parent.as_str(), "/");
    }

    #[test]
    fn cloud_path_parent_of_root() {
        let root = CloudPath::root();
        assert!(root.parent().is_none());
    }

    #[test]
    fn cloud_path_file_name() {
        let path = CloudPath::new("/documents/report.pdf");
        assert_eq!(path.file_name(), Some("report.pdf"));
    }

    #[test]
    fn cloud_path_file_name_of_root() {
        let root = CloudPath::root();
        assert!(root.file_name().is_none());
    }

    #[test]
    fn cloud_path_join() {
        let base = CloudPath::new("/documents");
        let joined = base.join("subfolder/file.txt");
        assert_eq!(joined.as_str(), "/documents/subfolder/file.txt");
    }

    #[test]
    fn provider_id_as_str() {
        assert_eq!(ProviderId::GoogleDrive.as_str(), "gdrive");
        assert_eq!(ProviderId::Dropbox.as_str(), "dropbox");
        assert_eq!(ProviderId::OneDrive.as_str(), "onedrive");
    }

    #[test]
    fn provider_id_display_name() {
        assert_eq!(ProviderId::GoogleDrive.display_name(), "Google Drive");
        assert_eq!(ProviderId::Dropbox.display_name(), "Dropbox");
        assert_eq!(ProviderId::OneDrive.display_name(), "OneDrive");
    }

    #[test]
    fn cloud_item_folder_creation() {
        let item = CloudItem::folder(
            FileId::new("folder123"),
            "Documents".to_string(),
            CloudPath::new("/Documents"),
            Utc::now(),
        );
        assert!(item.is_folder);
        assert!(item.size.is_none());
    }

    #[test]
    fn cloud_item_file_creation() {
        let item = CloudItem::file(
            FileId::new("file123"),
            "report.pdf".to_string(),
            CloudPath::new("/Documents/report.pdf"),
            1024,
            Utc::now(),
        );
        assert!(!item.is_folder);
        assert_eq!(item.size, Some(1024));
    }

    #[test]
    fn transfer_progress_percentage() {
        let progress = TransferProgress {
            bytes_transferred: 50,
            total_bytes: 100,
        };
        assert!((progress.percentage() - 50.0).abs() < 0.001);
    }

    #[test]
    fn transfer_progress_percentage_zero_total() {
        let progress = TransferProgress {
            bytes_transferred: 0,
            total_bytes: 0,
        };
        assert!((progress.percentage() - 100.0).abs() < 0.001);
    }

    #[test]
    fn transfer_progress_is_complete() {
        let incomplete = TransferProgress {
            bytes_transferred: 50,
            total_bytes: 100,
        };
        assert!(!incomplete.is_complete());

        let complete = TransferProgress {
            bytes_transferred: 100,
            total_bytes: 100,
        };
        assert!(complete.is_complete());
    }

    #[test]
    fn share_options_default() {
        let opts = ShareOptions::default();
        assert!(!opts.allow_edit);
        assert!(opts.expires_at.is_none());
        assert!(opts.password.is_none());
    }

    #[test]
    fn provider_id_serializes_to_lowercase() {
        let json = serde_json::to_string(&ProviderId::GoogleDrive).unwrap();
        assert_eq!(json, "\"googledrive\"");
    }

    #[tokio::test]
    async fn progress_channel_works() {
        let (tx, mut rx) = progress_channel(10);

        let progress = TransferProgress {
            bytes_transferred: 512,
            total_bytes: 1024,
        };

        tx.send(progress).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.bytes_transferred, 512);
        assert_eq!(received.total_bytes, 1024);
    }
}
