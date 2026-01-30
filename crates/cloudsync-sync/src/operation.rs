//! Sync operation types and definitions.

use chrono::{DateTime, Utc};
use cloudsync_core::types::{AccountId, CloudPath, FileId};
use serde::{Deserialize, Serialize};

/// Type of sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncOperationType {
    /// Upload a file to the cloud.
    Upload,

    /// Download a file from the cloud.
    Download,

    /// Delete a file (local or cloud).
    Delete,
}

impl std::fmt::Display for SyncOperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncOperationType::Upload => write!(f, "upload"),
            SyncOperationType::Download => write!(f, "download"),
            SyncOperationType::Delete => write!(f, "delete"),
        }
    }
}

/// Priority level for sync operations.
///
/// User-initiated operations (like "download now" from context menu)
/// get higher priority than background sync operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncPriority {
    /// Low priority - background sync, deferred operations.
    Low = 0,

    /// Normal priority - regular sync operations.
    Normal = 1,

    /// High priority - user-initiated operations.
    High = 2,
}

impl std::fmt::Display for SyncPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncPriority::Low => write!(f, "low"),
            SyncPriority::Normal => write!(f, "normal"),
            SyncPriority::High => write!(f, "high"),
        }
    }
}

/// Unique identifier for a sync operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(pub String);

impl OperationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents a sync operation (upload, download, or delete).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    /// Unique identifier for this operation.
    pub id: OperationId,

    /// Account this operation belongs to.
    pub account_id: AccountId,

    /// File ID from the provider.
    pub file_id: FileId,

    /// Path in the cloud filesystem.
    pub path: CloudPath,

    /// Type of operation.
    pub operation_type: SyncOperationType,

    /// Priority level.
    pub priority: SyncPriority,

    /// When this operation was created.
    pub created_at: DateTime<Utc>,

    /// File size (if known, for progress tracking).
    pub size: Option<u64>,
}

impl SyncOperation {
    /// Creates a new sync operation.
    pub fn new(
        account_id: AccountId,
        file_id: FileId,
        path: CloudPath,
        operation_type: SyncOperationType,
        priority: SyncPriority,
        size: Option<u64>,
    ) -> Self {
        Self {
            id: OperationId::new(),
            account_id,
            file_id,
            path,
            operation_type,
            priority,
            created_at: Utc::now(),
            size,
        }
    }

    /// Creates a high-priority upload operation (user-initiated).
    pub fn upload_high_priority(
        account_id: AccountId,
        file_id: FileId,
        path: CloudPath,
        size: Option<u64>,
    ) -> Self {
        Self::new(
            account_id,
            file_id,
            path,
            SyncOperationType::Upload,
            SyncPriority::High,
            size,
        )
    }

    /// Creates a normal-priority download operation.
    pub fn download_normal_priority(
        account_id: AccountId,
        file_id: FileId,
        path: CloudPath,
        size: Option<u64>,
    ) -> Self {
        Self::new(
            account_id,
            file_id,
            path,
            SyncOperationType::Download,
            SyncPriority::Normal,
            size,
        )
    }

    /// Creates a high-priority download operation (user-initiated).
    pub fn download_high_priority(
        account_id: AccountId,
        file_id: FileId,
        path: CloudPath,
        size: Option<u64>,
    ) -> Self {
        Self::new(
            account_id,
            file_id,
            path,
            SyncOperationType::Download,
            SyncPriority::High,
            size,
        )
    }

    /// Creates a delete operation.
    pub fn delete(
        account_id: AccountId,
        file_id: FileId,
        path: CloudPath,
        priority: SyncPriority,
    ) -> Self {
        Self::new(
            account_id,
            file_id,
            path,
            SyncOperationType::Delete,
            priority,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_id_generates_unique_ids() {
        let id1 = OperationId::new();
        let id2 = OperationId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn operation_id_from_string() {
        let id = OperationId::from_string("test-op-123");
        assert_eq!(id.0, "test-op-123");
        assert_eq!(format!("{}", id), "test-op-123");
    }

    #[test]
    fn sync_operation_type_display() {
        assert_eq!(format!("{}", SyncOperationType::Upload), "upload");
        assert_eq!(format!("{}", SyncOperationType::Download), "download");
        assert_eq!(format!("{}", SyncOperationType::Delete), "delete");
    }

    #[test]
    fn sync_priority_ordering() {
        assert!(SyncPriority::High > SyncPriority::Normal);
        assert!(SyncPriority::Normal > SyncPriority::Low);
        assert!(SyncPriority::High > SyncPriority::Low);
    }

    #[test]
    fn sync_priority_display() {
        assert_eq!(format!("{}", SyncPriority::Low), "low");
        assert_eq!(format!("{}", SyncPriority::Normal), "normal");
        assert_eq!(format!("{}", SyncPriority::High), "high");
    }

    #[test]
    fn sync_operation_creation() {
        let account_id = AccountId::new();
        let file_id = FileId::new("file123");
        let path = CloudPath::new("/documents/test.pdf");

        let op = SyncOperation::new(
            account_id.clone(),
            file_id.clone(),
            path.clone(),
            SyncOperationType::Download,
            SyncPriority::High,
            Some(1024),
        );

        assert_eq!(op.account_id, account_id);
        assert_eq!(op.file_id, file_id);
        assert_eq!(op.path, path);
        assert_eq!(op.operation_type, SyncOperationType::Download);
        assert_eq!(op.priority, SyncPriority::High);
        assert_eq!(op.size, Some(1024));
        assert!(op.created_at <= Utc::now());
    }

    #[test]
    fn upload_high_priority_constructor() {
        let account_id = AccountId::new();
        let file_id = FileId::new("file123");
        let path = CloudPath::new("/documents/test.pdf");

        let op = SyncOperation::upload_high_priority(
            account_id.clone(),
            file_id.clone(),
            path.clone(),
            Some(2048),
        );

        assert_eq!(op.operation_type, SyncOperationType::Upload);
        assert_eq!(op.priority, SyncPriority::High);
        assert_eq!(op.size, Some(2048));
    }

    #[test]
    fn download_normal_priority_constructor() {
        let account_id = AccountId::new();
        let file_id = FileId::new("file123");
        let path = CloudPath::new("/documents/test.pdf");

        let op = SyncOperation::download_normal_priority(
            account_id.clone(),
            file_id.clone(),
            path.clone(),
            Some(1024),
        );

        assert_eq!(op.operation_type, SyncOperationType::Download);
        assert_eq!(op.priority, SyncPriority::Normal);
    }

    #[test]
    fn download_high_priority_constructor() {
        let account_id = AccountId::new();
        let file_id = FileId::new("file123");
        let path = CloudPath::new("/documents/test.pdf");

        let op = SyncOperation::download_high_priority(
            account_id.clone(),
            file_id.clone(),
            path.clone(),
            Some(512),
        );

        assert_eq!(op.operation_type, SyncOperationType::Download);
        assert_eq!(op.priority, SyncPriority::High);
    }

    #[test]
    fn delete_constructor() {
        let account_id = AccountId::new();
        let file_id = FileId::new("file123");
        let path = CloudPath::new("/documents/test.pdf");

        let op = SyncOperation::delete(
            account_id.clone(),
            file_id.clone(),
            path.clone(),
            SyncPriority::Normal,
        );

        assert_eq!(op.operation_type, SyncOperationType::Delete);
        assert_eq!(op.priority, SyncPriority::Normal);
        assert_eq!(op.size, None);
    }

    #[test]
    fn operation_type_serializes_to_lowercase() {
        let json = serde_json::to_string(&SyncOperationType::Upload).unwrap();
        assert_eq!(json, "\"upload\"");
    }

    #[test]
    fn priority_serializes_to_lowercase() {
        let json = serde_json::to_string(&SyncPriority::High).unwrap();
        assert_eq!(json, "\"high\"");
    }
}
