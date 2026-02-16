//! Path resolution between local filesystem paths and cloud paths.
//!
//! Shell extensions send absolute local paths (e.g., `/home/user/UniCloudST/gdrive/docs/report.pdf`),
//! but the database stores cloud-relative paths (`/docs/report.pdf`). This module bridges
//! that gap by stripping the sync root and provider slug prefix.

use cloudsync_core::types::CloudPath;
use std::path::{Path, PathBuf};

/// Result of resolving a local path into its cloud-relative components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    /// The provider slug extracted from the path (e.g., "gdrive", "dropbox").
    pub provider_slug: String,

    /// The cloud-relative path within the provider's namespace.
    pub cloud_path: CloudPath,
}

/// Resolves local filesystem paths to cloud-relative paths and vice versa.
///
/// The expected directory layout is:
/// ```text
/// {sync_root}/{provider_slug}/{cloud_path...}
/// ```
///
/// For example, with sync root `/home/user/UniCloudST`:
/// - `/home/user/UniCloudST/gdrive/docs/report.pdf` resolves to
///   provider `gdrive`, cloud path `/docs/report.pdf`
#[derive(Debug, Clone)]
pub struct PathResolver {
    sync_root: PathBuf,
}

impl PathResolver {
    pub fn new(sync_root: impl Into<PathBuf>) -> Self {
        Self {
            sync_root: sync_root.into(),
        }
    }

    /// Returns the configured sync root path.
    pub fn sync_root(&self) -> &Path {
        &self.sync_root
    }

    /// Resolves an absolute local path into a provider slug and cloud path.
    ///
    /// Returns `None` if the path is outside the sync root or has no provider component.
    pub fn resolve_local(&self, local_path: &Path) -> Option<ResolvedPath> {
        let relative = local_path.strip_prefix(&self.sync_root).ok()?;

        let mut components = relative.components();
        let provider_slug = components.next()?.as_os_str().to_str()?.to_string();

        let remainder: PathBuf = components.collect();
        let cloud_path = if remainder.as_os_str().is_empty() {
            CloudPath::root()
        } else {
            CloudPath::new(format!("/{}", remainder.to_str()?))
        };

        Some(ResolvedPath {
            provider_slug,
            cloud_path,
        })
    }

    /// Reconstructs a local filesystem path from a provider slug and cloud path.
    pub fn to_local(&self, provider_slug: &str, cloud_path: &CloudPath) -> PathBuf {
        let mut path = self.sync_root.join(provider_slug);
        let cloud_str = cloud_path.as_str().trim_start_matches('/');
        if !cloud_str.is_empty() {
            path = path.join(cloud_str);
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> PathResolver {
        PathResolver::new("/home/user/UniCloudST")
    }

    #[test]
    fn resolve_valid_file_path() {
        let resolved = resolver()
            .resolve_local(Path::new("/home/user/UniCloudST/gdrive/docs/report.pdf"))
            .unwrap();

        assert_eq!(resolved.provider_slug, "gdrive");
        assert_eq!(resolved.cloud_path.as_str(), "/docs/report.pdf");
    }

    #[test]
    fn resolve_path_outside_sync_root_returns_none() {
        let result = resolver().resolve_local(Path::new("/tmp/other/file.txt"));
        assert!(result.is_none());
    }

    #[test]
    fn resolve_provider_root_returns_cloud_root() {
        let resolved = resolver()
            .resolve_local(Path::new("/home/user/UniCloudST/gdrive"))
            .unwrap();

        assert_eq!(resolved.provider_slug, "gdrive");
        assert!(resolved.cloud_path.is_root());
    }

    #[test]
    fn resolve_sync_root_itself_returns_none() {
        let result = resolver().resolve_local(Path::new("/home/user/UniCloudST"));
        assert!(result.is_none());
    }

    #[test]
    fn to_local_reconstructs_path() {
        let local = resolver().to_local("gdrive", &CloudPath::new("/docs/report.pdf"));
        assert_eq!(
            local,
            PathBuf::from("/home/user/UniCloudST/gdrive/docs/report.pdf")
        );
    }

    #[test]
    fn to_local_with_root_cloud_path() {
        let local = resolver().to_local("dropbox", &CloudPath::root());
        assert_eq!(local, PathBuf::from("/home/user/UniCloudST/dropbox"));
    }

    #[test]
    fn roundtrip_resolve_then_to_local() {
        let original = PathBuf::from("/home/user/UniCloudST/onedrive/work/notes.md");
        let resolved = resolver().resolve_local(&original).unwrap();
        let reconstructed = resolver().to_local(&resolved.provider_slug, &resolved.cloud_path);
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn resolve_nested_directory_path() {
        let resolved = resolver()
            .resolve_local(Path::new("/home/user/UniCloudST/gdrive/a/b/c/d/file.txt"))
            .unwrap();

        assert_eq!(resolved.provider_slug, "gdrive");
        assert_eq!(resolved.cloud_path.as_str(), "/a/b/c/d/file.txt");
    }

    #[test]
    fn different_providers_resolve_independently() {
        let r = resolver();

        let gdrive = r
            .resolve_local(Path::new("/home/user/UniCloudST/gdrive/file.txt"))
            .unwrap();
        let dropbox = r
            .resolve_local(Path::new("/home/user/UniCloudST/dropbox/file.txt"))
            .unwrap();

        assert_eq!(gdrive.provider_slug, "gdrive");
        assert_eq!(dropbox.provider_slug, "dropbox");
        assert_eq!(gdrive.cloud_path, dropbox.cloud_path);
    }
}
