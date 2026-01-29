//! CloudProvider implementation for Google Drive.
//!
//! This module implements the CloudProvider trait using the google-drive3 API.

use async_trait::async_trait;
use cloudsync_core::{
    error::{Error, Result},
    provider::CloudProvider,
    types::{
        ChangeList, CloudItem, CloudPath, FileId, FileVersion, ProgressSender,
        ShareOptions,
    },
};
use google_drive3::{hyper_rustls, hyper_util, yup_oauth2, DriveHub};
use std::path::Path;
use url::Url;

use super::client::GoogleDriveClient;

/// Google Drive implementation of CloudProvider.
///
/// This wraps GoogleDriveClient and provides the full CloudProvider interface
/// using the google-drive3 API.
pub struct GoogleDriveProvider {
    client: GoogleDriveClient,
    hub: DriveHub<hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>>,
}

impl GoogleDriveProvider {
    /// Creates a new Google Drive provider from a client.
    ///
    /// This creates the DriveHub instance needed for API calls.
    pub async fn new(client: GoogleDriveClient) -> Result<Self> {
        // Create HTTP client for DriveHub following google-drive3 documentation
        let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| Error::Config(format!("Failed to create HTTPS connector: {}", e)))?
            .https_or_http()
            .enable_http2()
            .build();

        let http_client = hyper_util::client::legacy::Client::builder(
            hyper_util::rt::TokioExecutor::new()
        ).build(https_connector);

        let hub = DriveHub::new(http_client, client.authenticator().clone());

        Ok(Self { client, hub })
    }

    /// Converts a Google Drive file resource to a CloudItem.
    fn file_to_cloud_item(file: &google_drive3::api::File) -> Result<CloudItem> {
        // Extract required fields with helpful error messages
        let id = file
            .id
            .as_ref()
            .ok_or_else(|| Error::ProviderApi {
                provider: "gdrive".to_string(),
                message: "File missing ID field".to_string(),
            })?;

        let name = file
            .name
            .as_ref()
            .ok_or_else(|| Error::ProviderApi {
                provider: "gdrive".to_string(),
                message: "File missing name field".to_string(),
            })?;

        // Determine if this is a folder
        let is_folder = file
            .mime_type
            .as_ref()
            .map(|mt| mt == "application/vnd.google-apps.folder")
            .unwrap_or(false);

        // Get modified time (required field in Drive API)
        let modified = file
            .modified_time
            .as_ref()
            .ok_or_else(|| Error::ProviderApi {
                provider: "gdrive".to_string(),
                message: "File missing modified_time field".to_string(),
            })?
            .clone();

        // Convert to CloudItem
        // Note: Google Drive doesn't have a traditional path hierarchy
        // We'll construct the path from the file name for now
        let path = CloudPath::new(format!("/{}", name));

        Ok(CloudItem {
            id: FileId::new(id.clone()),
            name: name.clone(),
            path,
            is_folder,
            size: file.size.map(|s| s as u64),
            content_hash: file.md5_checksum.clone(),
            modified,
            created: file.created_time.clone(),
            mime_type: file.mime_type.clone(),
        })
    }
}

#[async_trait]
impl CloudProvider for GoogleDriveProvider {
    fn id(&self) -> &'static str {
        "gdrive"
    }

    fn display_name(&self) -> &'static str {
        "Google Drive"
    }

    async fn authenticate(&mut self, _auth_code: &str) -> Result<()> {
        // Authentication is handled by GoogleDriveClient during construction
        // This method is a no-op since OAuth flow happens in GoogleDriveClient::new
        // TODO: Consider if we need to handle re-authentication here
        Ok(())
    }

    async fn refresh_token(&mut self) -> Result<()> {
        // Token refresh is handled automatically by yup-oauth2 authenticator
        // Force a token refresh by requesting a new token
        self.client
            .get_token()
            .await
            .map_err(|e| Error::Authentication(format!("Failed to refresh token: {}", e)))?;
        Ok(())
    }

    async fn list_folder(&self, _path: &CloudPath) -> Result<Vec<CloudItem>> {
        // TODO: Implement folder listing using Files.list API
        // For now, return empty list
        Ok(vec![])
    }

    async fn download(
        &self,
        _id: &FileId,
        _dest: &Path,
        _progress: Option<ProgressSender>,
    ) -> Result<()> {
        // TODO: Implement file download using Files.get with alt=media
        Err(Error::InvalidOperation(
            "Download not yet implemented".to_string(),
        ))
    }

    async fn upload(
        &self,
        _src: &Path,
        _dest: &CloudPath,
        _progress: Option<ProgressSender>,
    ) -> Result<CloudItem> {
        // TODO: Implement file upload using Files.create
        Err(Error::InvalidOperation(
            "Upload not yet implemented".to_string(),
        ))
    }

    async fn delete(&self, _id: &FileId) -> Result<()> {
        // TODO: Implement file deletion using Files.delete
        Err(Error::InvalidOperation(
            "Delete not yet implemented".to_string(),
        ))
    }

    async fn move_item(&self, _id: &FileId, _new_path: &CloudPath) -> Result<()> {
        // TODO: Implement file move using Files.update
        Err(Error::InvalidOperation(
            "Move not yet implemented".to_string(),
        ))
    }

    async fn get_changes(&self, _cursor: Option<&str>) -> Result<ChangeList> {
        // TODO: Implement changes feed using Changes.list API
        Err(Error::InvalidOperation(
            "Get changes not yet implemented".to_string(),
        ))
    }

    async fn create_share_link(
        &self,
        _id: &FileId,
        _options: ShareOptions,
    ) -> Result<Url> {
        // TODO: Implement share link creation using Permissions.create
        Err(Error::InvalidOperation(
            "Create share link not yet implemented".to_string(),
        ))
    }

    async fn get_metadata(&self, _id: &FileId) -> Result<CloudItem> {
        // TODO: Implement metadata retrieval using Files.get
        Err(Error::InvalidOperation(
            "Get metadata not yet implemented".to_string(),
        ))
    }

    async fn get_versions(&self, _id: &FileId) -> Result<Vec<FileVersion>> {
        // TODO: Implement version history using Revisions.list API
        Err(Error::InvalidOperation(
            "Get versions not yet implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdrive::oauth::OAuthConfig;
    use std::path::PathBuf;
    use yup_oauth2::InstalledFlowReturnMethod;

    async fn create_test_provider() -> GoogleDriveProvider {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let client = GoogleDriveClient::new(
            config,
            Some(PathBuf::from("/tmp/test-gdrive-tokens.json")),
            InstalledFlowReturnMethod::HTTPRedirect,
        )
        .await
        .expect("Failed to create client");

        GoogleDriveProvider::new(client)
            .await
            .expect("Failed to create provider")
    }

    #[tokio::test]
    async fn provider_has_correct_id() {
        let provider = create_test_provider().await;
        assert_eq!(provider.id(), "gdrive");
    }

    #[tokio::test]
    async fn provider_has_correct_display_name() {
        let provider = create_test_provider().await;
        assert_eq!(provider.display_name(), "Google Drive");
    }

    #[tokio::test]
    async fn provider_authenticate_succeeds() {
        let mut provider = create_test_provider().await;
        // Since authentication is handled in GoogleDriveClient,
        // this should be a no-op
        let result = provider.authenticate("dummy-code").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn provider_list_folder_returns_empty() {
        let provider = create_test_provider().await;
        let result = provider.list_folder(&CloudPath::root()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn provider_download_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider
            .download(
                &FileId::new("test-id"),
                Path::new("/tmp/test-file"),
                None,
            )
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_upload_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider
            .upload(
                Path::new("/tmp/test-file"),
                &CloudPath::new("/test-file"),
                None,
            )
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_delete_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider.delete(&FileId::new("test-id")).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_move_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider
            .move_item(&FileId::new("test-id"), &CloudPath::new("/new-path"))
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_get_changes_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider.get_changes(None).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_create_share_link_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider
            .create_share_link(&FileId::new("test-id"), ShareOptions::default())
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_get_metadata_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider.get_metadata(&FileId::new("test-id")).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn provider_get_versions_returns_not_implemented() {
        let provider = create_test_provider().await;
        let result = provider.get_versions(&FileId::new("test-id")).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidOperation(_)));
    }
}
