//! Local content cache for on-demand file downloads.
//!
//! `ContentCache` manages downloading cloud files to a local cache directory
//! and serving reads from the cached copies. Downloads are triggered on
//! `open()` and reads are served from local disk via `read()`.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use chrono::Utc;
use cloudsync_core::error::Error as CloudError;
use cloudsync_core::file_state::FileState;
use cloudsync_core::provider::CloudProvider;
use cloudsync_core::types::{AccountId, FileId};
use cloudsync_db::Database;
use tracing::{error, info, warn};

/// Status of an in-progress download for a specific file.
#[derive(Debug, Clone, PartialEq)]
enum DownloadStatus {
    InProgress,
    Complete,
    Failed(String),
}

/// Per-file download lock: waiters block on the Condvar until the download completes.
type DownloadLock = Arc<(Mutex<DownloadStatus>, Condvar)>;

/// Manages local caching of cloud file content.
///
/// Files are stored in a flat layout: `{cache_root}/{file_id}`.
/// Per-file locking prevents concurrent duplicate downloads.
pub struct ContentCache {
    cache_root: PathBuf,
    provider: Arc<dyn CloudProvider>,
    runtime: tokio::runtime::Runtime,
    db: Database,
    account_id: AccountId,
    download_locks: Mutex<HashMap<String, DownloadLock>>,
}

impl ContentCache {
    /// Create a new content cache.
    ///
    /// Creates a dedicated tokio runtime for async downloads and ensures
    /// the cache directory exists.
    pub fn new(
        cache_root: PathBuf,
        provider: Arc<dyn CloudProvider>,
        db: Database,
        account_id: AccountId,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&cache_root)?;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;

        Ok(Self {
            cache_root,
            provider,
            runtime,
            db,
            account_id,
            download_locks: Mutex::new(HashMap::new()),
        })
    }

    /// Returns the local cache path for a given file ID.
    pub fn cached_path(&self, file_id: &FileId) -> PathBuf {
        self.cache_root.join(file_id.to_string())
    }

    /// Ensures the file is downloaded locally, returning the cached path.
    ///
    /// If the file is already cached on disk, returns immediately.
    /// If another thread is downloading, waits for completion.
    /// On success, updates the DB state to `Synced`.
    ///
    /// # Errors
    /// Returns a libc error code on failure:
    /// - `ENOTSUP` for cloud-native files (Google Docs)
    /// - `EIO` for download or I/O errors
    pub fn ensure_downloaded(&self, file_id: &FileId) -> Result<PathBuf, i32> {
        let path = self.cached_path(file_id);

        // Fast path: already cached on disk
        if path.exists() {
            return Ok(path);
        }

        // Acquire per-file lock
        let lock = {
            let mut locks = self.download_locks.lock().map_err(|_| libc::EIO)?;
            locks
                .entry(file_id.to_string())
                .or_insert_with(|| {
                    Arc::new((Mutex::new(DownloadStatus::InProgress), Condvar::new()))
                })
                .clone()
        };

        let (status_mutex, condvar) = &*lock;
        let mut status = status_mutex.lock().map_err(|_| libc::EIO)?;

        // Check if another thread already completed the download while we waited
        if path.exists() {
            return Ok(path);
        }

        match &*status {
            DownloadStatus::InProgress => {
                // We're the first thread — do the download
            }
            DownloadStatus::Complete => {
                return Ok(path);
            }
            DownloadStatus::Failed(msg) => {
                warn!("Previous download failed for {}: {}", file_id, msg);
                // Retry the download
            }
        }

        *status = DownloadStatus::InProgress;
        drop(status);

        // Update DB state to Syncing
        self.update_file_state(file_id, FileState::Syncing);

        info!("Downloading file {} to {:?}", file_id, path);

        // Perform the async download on our dedicated runtime
        let provider = self.provider.clone();
        let file_id_clone = file_id.clone();
        let dest = path.clone();

        let result = self
            .runtime
            .block_on(provider.download(&file_id_clone, &dest, None));

        let mut status = status_mutex.lock().map_err(|_| libc::EIO)?;

        match result {
            Ok(()) => {
                *status = DownloadStatus::Complete;
                condvar.notify_all();

                self.update_file_state(file_id, FileState::Synced);
                self.update_synced_at(file_id);

                info!("Downloaded file {} successfully", file_id);
                Ok(path)
            }
            Err(CloudError::CloudNativeFile { url }) => {
                *status = DownloadStatus::Failed("cloud-native file".to_string());
                condvar.notify_all();

                warn!(
                    "File {} is a cloud-native file (open in browser): {}",
                    file_id, url
                );
                Err(libc::ENOTSUP)
            }
            Err(e) => {
                let msg = e.to_string();
                *status = DownloadStatus::Failed(msg.clone());
                condvar.notify_all();

                self.update_file_state(file_id, FileState::Error);

                error!("Failed to download file {}: {}", file_id, msg);
                Err(libc::EIO)
            }
        }
    }

    /// Read bytes from a cached file at the given offset.
    ///
    /// # Errors
    /// Returns `EIO` if the file cannot be read.
    pub fn read_file_at(&self, path: &Path, offset: i64, size: u32) -> Result<Vec<u8>, i32> {
        let mut file = std::fs::File::open(path).map_err(|e| {
            error!("Failed to open cached file {:?}: {}", path, e);
            libc::EIO
        })?;

        file.seek(SeekFrom::Start(offset as u64)).map_err(|e| {
            error!("Failed to seek in cached file {:?}: {}", path, e);
            libc::EIO
        })?;

        let mut buf = vec![0u8; size as usize];
        let bytes_read = file.read(&mut buf).map_err(|e| {
            error!("Failed to read cached file {:?}: {}", path, e);
            libc::EIO
        })?;

        buf.truncate(bytes_read);
        Ok(buf)
    }

    /// Update the file state in the database.
    fn update_file_state(&self, file_id: &FileId, state: FileState) {
        let result = self.db.with_conn(|conn| {
            let file = cloudsync_db::get_file_by_provider_id(conn, &self.account_id, file_id)?;
            if let Some(mut db_file) = file {
                db_file.state = state;
                cloudsync_db::update_file(conn, &db_file)?;
            }
            Ok(())
        });

        if let Err(e) = result {
            warn!("Failed to update file state for {}: {}", file_id, e);
        }
    }

    /// Set the `synced_at` timestamp for a file.
    fn update_synced_at(&self, file_id: &FileId) {
        let result = self.db.with_conn(|conn| {
            let file = cloudsync_db::get_file_by_provider_id(conn, &self.account_id, file_id)?;
            if let Some(mut db_file) = file {
                db_file.synced_at = Some(Utc::now());
                cloudsync_db::update_file(conn, &db_file)?;
            }
            Ok(())
        });

        if let Err(e) = result {
            warn!("Failed to update synced_at for {}: {}", file_id, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use cloudsync_core::types::{
        ChangeList, CloudItem, CloudPath, FileVersion, ProviderId, ShareOptions, TransferProgress,
    };
    use cloudsync_db::{
        accounts, create_file, Migration, ACCOUNTS_MIGRATION, FILES_MIGRATION,
        SYNC_CURSORS_MIGRATION, VFS_INODES_MIGRATION,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use url::Url;

    /// Mock provider that supports download with configurable content.
    struct MockDownloadProvider {
        download_content: HashMap<String, Vec<u8>>,
        cloud_native_ids: Vec<String>,
        error_ids: Vec<String>,
        download_count: AtomicUsize,
    }

    impl MockDownloadProvider {
        fn new() -> Self {
            Self {
                download_content: HashMap::new(),
                cloud_native_ids: Vec::new(),
                error_ids: Vec::new(),
                download_count: AtomicUsize::new(0),
            }
        }

        fn with_file(mut self, id: &str, content: &[u8]) -> Self {
            self.download_content
                .insert(id.to_string(), content.to_vec());
            self
        }

        fn with_cloud_native(mut self, id: &str) -> Self {
            self.cloud_native_ids.push(id.to_string());
            self
        }

        fn with_error(mut self, id: &str) -> Self {
            self.error_ids.push(id.to_string());
            self
        }
    }

    #[async_trait]
    impl CloudProvider for MockDownloadProvider {
        fn id(&self) -> &'static str {
            "mock"
        }
        fn display_name(&self) -> &'static str {
            "Mock Provider"
        }
        async fn authenticate(&mut self, _: &str) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }
        async fn refresh_token(&mut self) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }
        async fn list_folder(
            &self,
            _: &CloudPath,
        ) -> cloudsync_core::error::Result<Vec<CloudItem>> {
            unimplemented!()
        }
        async fn download(
            &self,
            id: &FileId,
            dest: &Path,
            _progress: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        ) -> cloudsync_core::error::Result<()> {
            self.download_count.fetch_add(1, Ordering::SeqCst);

            let id_str = id.to_string();

            if self.cloud_native_ids.contains(&id_str) {
                return Err(CloudError::CloudNativeFile {
                    url: format!("https://docs.google.com/document/d/{}", id_str),
                });
            }

            if self.error_ids.contains(&id_str) {
                return Err(CloudError::Network(
                    "simulated download failure".to_string(),
                ));
            }

            if let Some(content) = self.download_content.get(&id_str) {
                std::fs::write(dest, content)?;
                Ok(())
            } else {
                Err(CloudError::FileNotFound { path: id_str })
            }
        }
        async fn upload(
            &self,
            _: &Path,
            _: &CloudPath,
            _: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        ) -> cloudsync_core::error::Result<CloudItem> {
            unimplemented!()
        }
        async fn delete(&self, _: &FileId) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }
        async fn move_item(&self, _: &FileId, _: &CloudPath) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }
        async fn get_changes(&self, _: Option<&str>) -> cloudsync_core::error::Result<ChangeList> {
            unimplemented!()
        }
        async fn create_share_link(
            &self,
            _: &FileId,
            _: ShareOptions,
        ) -> cloudsync_core::error::Result<Url> {
            unimplemented!()
        }
        async fn get_metadata(&self, _: &FileId) -> cloudsync_core::error::Result<CloudItem> {
            unimplemented!()
        }
        async fn get_versions(
            &self,
            _: &FileId,
        ) -> cloudsync_core::error::Result<Vec<FileVersion>> {
            unimplemented!()
        }
    }

    fn setup_test_db() -> (Database, AccountId) {
        let db = Database::in_memory_with_migrations(vec![
            Migration {
                version: 1,
                description: "Create accounts table",
                sql: ACCOUNTS_MIGRATION,
            },
            Migration {
                version: 2,
                description: "Create files table",
                sql: FILES_MIGRATION,
            },
            Migration {
                version: 3,
                description: "Create vfs_inodes table",
                sql: VFS_INODES_MIGRATION,
            },
            Migration {
                version: 4,
                description: "Create sync_cursors table",
                sql: SYNC_CURSORS_MIGRATION,
            },
        ])
        .unwrap();

        let account_id = db
            .with_conn(|conn| {
                let account = accounts::Account::new(
                    ProviderId::GoogleDrive,
                    "test@example.com".to_string(),
                    "token".to_string(),
                    None,
                    None,
                );
                accounts::create_account(conn, &account)?;
                Ok(account.id)
            })
            .unwrap();

        (db, account_id)
    }

    fn insert_test_file(db: &Database, account_id: &AccountId, file_id: &str) {
        db.with_conn(|conn| {
            let mut file = cloudsync_db::File::new(
                account_id.clone(),
                FileId::new(file_id),
                CloudPath::new(format!("/{}.txt", file_id)),
                format!("{}.txt", file_id),
                Some(1024),
                None,
                false,
                Utc::now(),
            );
            file.state = FileState::CloudOnly;
            create_file(conn, &file)?;
            Ok(())
        })
        .unwrap();
    }

    fn get_file_state(db: &Database, account_id: &AccountId, file_id: &str) -> Option<FileState> {
        db.with_conn(|conn| {
            let file =
                cloudsync_db::get_file_by_provider_id(conn, account_id, &FileId::new(file_id))?;
            Ok(file.map(|f| f.state))
        })
        .unwrap()
    }

    #[test]
    fn test_ensure_downloaded_calls_provider() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();
        insert_test_file(&db, &account_id, "file1");

        let provider = Arc::new(MockDownloadProvider::new().with_file("file1", b"hello world"));

        let cache = ContentCache::new(tmp.path().to_path_buf(), provider, db, account_id).unwrap();

        let path = cache.ensure_downloaded(&FileId::new("file1")).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn test_ensure_downloaded_serves_from_cache() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();
        insert_test_file(&db, &account_id, "file1");

        let provider = Arc::new(MockDownloadProvider::new().with_file("file1", b"cached content"));

        let cache =
            ContentCache::new(tmp.path().to_path_buf(), provider.clone(), db, account_id).unwrap();

        // First download
        cache.ensure_downloaded(&FileId::new("file1")).unwrap();
        // Second call should serve from cache (no re-download)
        cache.ensure_downloaded(&FileId::new("file1")).unwrap();

        assert_eq!(provider.download_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_cloud_native_file_returns_enotsup() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();
        insert_test_file(&db, &account_id, "gdoc1");

        let provider = Arc::new(MockDownloadProvider::new().with_cloud_native("gdoc1"));

        let cache = ContentCache::new(tmp.path().to_path_buf(), provider, db, account_id).unwrap();

        let err = cache.ensure_downloaded(&FileId::new("gdoc1")).unwrap_err();
        assert_eq!(err, libc::ENOTSUP);
    }

    #[test]
    fn test_download_error_returns_eio() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();
        insert_test_file(&db, &account_id, "broken");

        let provider = Arc::new(MockDownloadProvider::new().with_error("broken"));

        let cache = ContentCache::new(tmp.path().to_path_buf(), provider, db, account_id).unwrap();

        let err = cache.ensure_downloaded(&FileId::new("broken")).unwrap_err();
        assert_eq!(err, libc::EIO);
    }

    #[test]
    fn test_read_file_at_correct_offset() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();

        let provider = Arc::new(MockDownloadProvider::new());
        let cache = ContentCache::new(tmp.path().to_path_buf(), provider, db, account_id).unwrap();

        // Write a known file directly
        let test_path = tmp.path().join("test_read");
        std::fs::write(&test_path, b"abcdefghijklmnop").unwrap();

        // Read from offset 4, size 5 → "efghi"
        let data = cache.read_file_at(&test_path, 4, 5).unwrap();
        assert_eq!(data, b"efghi");

        // Read from offset 0, size 3 → "abc"
        let data = cache.read_file_at(&test_path, 0, 3).unwrap();
        assert_eq!(data, b"abc");
    }

    #[test]
    fn test_read_file_at_past_eof() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();

        let provider = Arc::new(MockDownloadProvider::new());
        let cache = ContentCache::new(tmp.path().to_path_buf(), provider, db, account_id).unwrap();

        let test_path = tmp.path().join("short_file");
        std::fs::write(&test_path, b"abc").unwrap();

        // Read beyond EOF
        let data = cache.read_file_at(&test_path, 100, 10).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_db_state_transitions() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();
        insert_test_file(&db, &account_id, "file1");

        // Verify initial state
        assert_eq!(
            get_file_state(&db, &account_id, "file1"),
            Some(FileState::CloudOnly)
        );

        let provider = Arc::new(MockDownloadProvider::new().with_file("file1", b"content"));

        let cache = ContentCache::new(
            tmp.path().to_path_buf(),
            provider,
            db.clone(),
            account_id.clone(),
        )
        .unwrap();

        cache.ensure_downloaded(&FileId::new("file1")).unwrap();

        // After successful download, state should be Synced
        assert_eq!(
            get_file_state(&db, &account_id, "file1"),
            Some(FileState::Synced)
        );

        // synced_at should be set
        let synced_at = db
            .with_conn(|conn| {
                let file = cloudsync_db::get_file_by_provider_id(
                    conn,
                    &account_id,
                    &FileId::new("file1"),
                )?;
                Ok(file.and_then(|f| f.synced_at))
            })
            .unwrap();
        assert!(synced_at.is_some());
    }

    #[test]
    fn test_db_state_on_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (db, account_id) = setup_test_db();
        insert_test_file(&db, &account_id, "broken");

        let provider = Arc::new(MockDownloadProvider::new().with_error("broken"));

        let cache = ContentCache::new(
            tmp.path().to_path_buf(),
            provider,
            db.clone(),
            account_id.clone(),
        )
        .unwrap();

        let _ = cache.ensure_downloaded(&FileId::new("broken"));

        // After failed download, state should be Error
        assert_eq!(
            get_file_state(&db, &account_id, "broken"),
            Some(FileState::Error)
        );
    }
}
