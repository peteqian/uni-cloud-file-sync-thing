//! Cloud provider trait definition.
//!
//! This module will contain the `CloudProvider` trait that all cloud
//! storage backends must implement. Full implementation in Phase 1.2.

use std::path::Path;

use async_trait::async_trait;
use url::Url;

use crate::error::Result;
use crate::types::{ChangeList, CloudItem, CloudPath, FileId, FileVersion, ProgressSender, ShareOptions};

/// Trait that all cloud storage providers must implement.
///
/// This trait defines the common interface for interacting with cloud
/// storage services like Google Drive, Dropbox, and OneDrive.
#[async_trait]
pub trait CloudProvider: Send + Sync {
    /// Returns the provider identifier (e.g., "gdrive", "dropbox", "onedrive").
    fn id(&self) -> &'static str;

    /// Returns the human-readable provider name.
    fn display_name(&self) -> &'static str;

    /// Authenticates with the provider using an authorization code.
    async fn authenticate(&mut self, auth_code: &str) -> Result<()>;

    /// Refreshes expired tokens.
    async fn refresh_token(&mut self) -> Result<()>;

    /// Lists contents of a folder.
    async fn list_folder(&self, path: &CloudPath) -> Result<Vec<CloudItem>>;

    /// Downloads a file to a local path.
    async fn download(
        &self,
        id: &FileId,
        dest: &Path,
        progress: Option<ProgressSender>,
    ) -> Result<()>;

    /// Uploads a file from a local path.
    async fn upload(
        &self,
        src: &Path,
        dest: &CloudPath,
        progress: Option<ProgressSender>,
    ) -> Result<CloudItem>;

    /// Deletes a file or folder.
    async fn delete(&self, id: &FileId) -> Result<()>;

    /// Moves or renames a file.
    async fn move_item(&self, id: &FileId, new_path: &CloudPath) -> Result<()>;

    /// Gets changes since a cursor (for incremental sync).
    async fn get_changes(&self, cursor: Option<&str>) -> Result<ChangeList>;

    /// Creates a share link for a file.
    async fn create_share_link(&self, id: &FileId, options: ShareOptions) -> Result<Url>;

    /// Gets metadata for a file.
    async fn get_metadata(&self, id: &FileId) -> Result<CloudItem>;

    /// Gets version history for a file.
    async fn get_versions(&self, id: &FileId) -> Result<Vec<FileVersion>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trait object safety test - ensures CloudProvider can be used as dyn
    #[test]
    fn provider_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn CloudProvider) {}
    }
}
