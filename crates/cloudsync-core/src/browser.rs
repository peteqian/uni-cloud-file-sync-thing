//! Browser utilities for opening cloud-native files.
//!
//! This module provides functionality to detect cloud-native files (Google Docs, Sheets, etc.)
//! and open them in the user's default browser.

use crate::error::{Error, Result};
use crate::types::FileId;
use url::Url;

/// Represents a cloud-native file that should be opened in a browser.
#[derive(Debug, Clone)]
pub struct CloudNativeFile {
    /// The file ID.
    pub file_id: FileId,

    /// The provider identifier.
    pub provider: String,

    /// The MIME type of the file.
    pub mime_type: String,

    /// The web URL to open this file.
    pub web_url: Url,
}

impl CloudNativeFile {
    /// Creates a new cloud-native file reference.
    pub fn new(file_id: FileId, provider: String, mime_type: String, web_url: Url) -> Self {
        Self {
            file_id,
            provider,
            mime_type,
            web_url,
        }
    }

    /// Opens this file in the user's default browser.
    ///
    /// Returns an error if the browser cannot be opened.
    pub fn open_in_browser(&self) -> Result<()> {
        opener::open(self.web_url.as_str())
            .map_err(|e| Error::InvalidOperation(format!("Failed to open browser: {}", e)))
    }
}

/// Helper trait for providers to construct web URLs for cloud-native files.
pub trait CloudNativeUrlBuilder {
    /// Constructs a web URL for a file ID, or None if the file is not cloud-native.
    fn build_web_url(&self, file_id: &FileId, mime_type: &str) -> Option<Url>;

    /// Checks if a MIME type represents a cloud-native file for this provider.
    fn is_cloud_native(&self, mime_type: &str) -> bool;
}

/// Google Drive URL builder implementation.
pub struct GoogleDriveUrlBuilder;

impl GoogleDriveUrlBuilder {
    /// Base URL for Google Docs.
    const DOCS_BASE: &'static str = "https://docs.google.com/document/d";

    /// Base URL for Google Sheets.
    const SHEETS_BASE: &'static str = "https://docs.google.com/spreadsheets/d";

    /// Base URL for Google Slides.
    const SLIDES_BASE: &'static str = "https://docs.google.com/presentation/d";

    /// Base URL for Google Forms.
    const FORMS_BASE: &'static str = "https://docs.google.com/forms/d";

    /// Base URL for Google Drawings.
    const DRAWINGS_BASE: &'static str = "https://docs.google.com/drawings/d";

    /// Fallback to Drive viewer for unknown Google Apps files.
    const DRIVE_VIEW_BASE: &'static str = "https://drive.google.com/file/d";
}

impl CloudNativeUrlBuilder for GoogleDriveUrlBuilder {
    fn build_web_url(&self, file_id: &FileId, mime_type: &str) -> Option<Url> {
        if !self.is_cloud_native(mime_type) {
            return None;
        }

        let base_url = match mime_type {
            "application/vnd.google-apps.document" => Self::DOCS_BASE,
            "application/vnd.google-apps.spreadsheet" => Self::SHEETS_BASE,
            "application/vnd.google-apps.presentation" => Self::SLIDES_BASE,
            "application/vnd.google-apps.form" => Self::FORMS_BASE,
            "application/vnd.google-apps.drawing" => Self::DRAWINGS_BASE,
            _ => Self::DRIVE_VIEW_BASE,
        };

        let url_str = format!("{}/{}/edit", base_url, file_id.0);
        Url::parse(&url_str).ok()
    }

    fn is_cloud_native(&self, mime_type: &str) -> bool {
        mime_type.starts_with("application/vnd.google-apps.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_drive_detects_cloud_native_docs() {
        let builder = GoogleDriveUrlBuilder;
        assert!(builder.is_cloud_native("application/vnd.google-apps.document"));
        assert!(builder.is_cloud_native("application/vnd.google-apps.spreadsheet"));
        assert!(builder.is_cloud_native("application/vnd.google-apps.presentation"));
        assert!(builder.is_cloud_native("application/vnd.google-apps.form"));
        assert!(builder.is_cloud_native("application/vnd.google-apps.drawing"));
    }

    #[test]
    fn google_drive_ignores_regular_files() {
        let builder = GoogleDriveUrlBuilder;
        assert!(!builder.is_cloud_native("application/pdf"));
        assert!(!builder.is_cloud_native("image/jpeg"));
        assert!(!builder.is_cloud_native("video/mp4"));
    }

    #[test]
    fn google_drive_builds_correct_docs_url() {
        let builder = GoogleDriveUrlBuilder;
        let file_id = FileId::new("abc123");
        let url = builder
            .build_web_url(&file_id, "application/vnd.google-apps.document")
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://docs.google.com/document/d/abc123/edit"
        );
    }

    #[test]
    fn google_drive_builds_correct_sheets_url() {
        let builder = GoogleDriveUrlBuilder;
        let file_id = FileId::new("xyz789");
        let url = builder
            .build_web_url(&file_id, "application/vnd.google-apps.spreadsheet")
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://docs.google.com/spreadsheets/d/xyz789/edit"
        );
    }

    #[test]
    fn google_drive_builds_correct_slides_url() {
        let builder = GoogleDriveUrlBuilder;
        let file_id = FileId::new("slide123");
        let url = builder
            .build_web_url(&file_id, "application/vnd.google-apps.presentation")
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://docs.google.com/presentation/d/slide123/edit"
        );
    }

    #[test]
    fn google_drive_builds_fallback_url_for_unknown_google_apps() {
        let builder = GoogleDriveUrlBuilder;
        let file_id = FileId::new("unknown123");
        let url = builder
            .build_web_url(&file_id, "application/vnd.google-apps.unknown")
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://drive.google.com/file/d/unknown123/edit"
        );
    }

    #[test]
    fn google_drive_returns_none_for_regular_files() {
        let builder = GoogleDriveUrlBuilder;
        let file_id = FileId::new("abc123");
        assert!(builder.build_web_url(&file_id, "application/pdf").is_none());
    }

    #[test]
    fn cloud_native_file_creation() {
        let file_id = FileId::new("test123");
        let url = Url::parse("https://docs.google.com/document/d/test123/edit").unwrap();
        let file = CloudNativeFile::new(
            file_id.clone(),
            "gdrive".to_string(),
            "application/vnd.google-apps.document".to_string(),
            url.clone(),
        );

        assert_eq!(file.file_id, file_id);
        assert_eq!(file.provider, "gdrive");
        assert_eq!(file.mime_type, "application/vnd.google-apps.document");
        assert_eq!(file.web_url, url);
    }
}
