//! Metadata synchronization from cloud providers to the local database.
//!
//! `MetadataSyncer` fetches file/folder metadata from a cloud provider and
//! stores it in the `files` table so the VFS can render them. It supports
//! both full (initial) and incremental (change-based) sync.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use cloudsync_core::file_state::FileState;
use cloudsync_core::provider::CloudProvider;
use cloudsync_core::types::{AccountId, Change, CloudItem, CloudPath};
use cloudsync_db::{self, Database, File};
use tracing::{error, info, warn};

/// Synchronizes cloud provider metadata into the local database.
pub struct MetadataSyncer {
    provider: Arc<dyn CloudProvider>,
    db: Database,
    account_id: AccountId,
}

impl MetadataSyncer {
    pub fn new(provider: Arc<dyn CloudProvider>, db: Database, account_id: AccountId) -> Self {
        Self {
            provider,
            db,
            account_id,
        }
    }

    /// Performs a full sync: fetches all root-level items and stores them.
    ///
    /// Also fetches the initial change cursor for future incremental syncs.
    /// Returns the number of items synced.
    pub async fn initial_sync(&self) -> Result<usize> {
        info!(
            "Starting initial metadata sync for account {}",
            self.account_id
        );

        let items = self
            .provider
            .list_folder(&CloudPath::root())
            .await
            .context("Failed to list root folder")?;

        let count = items.len();

        for item in &items {
            self.upsert_cloud_item(item)?;
        }

        // Fetch initial cursor for incremental sync
        let change_list = self
            .provider
            .get_changes(None)
            .await
            .context("Failed to get initial change cursor")?;

        self.db.with_conn(|conn| {
            cloudsync_db::sync_cursors::upsert_cursor(
                conn,
                &self.account_id.to_string(),
                &change_list.cursor,
            )
        })?;

        info!("Initial sync complete: {} items synced", count);
        Ok(count)
    }

    /// Performs an incremental sync using the stored change cursor.
    ///
    /// Falls back to `initial_sync()` if no cursor is stored.
    /// Returns the number of changes applied.
    pub async fn incremental_sync(&self) -> Result<usize> {
        let cursor = self.db.with_conn(|conn| {
            cloudsync_db::sync_cursors::get_cursor(conn, &self.account_id.to_string())
        })?;

        let Some(mut cursor) = cursor else {
            warn!("No cursor found, falling back to initial sync");
            return self.initial_sync().await;
        };

        info!("Starting incremental sync for account {}", self.account_id);

        let mut total_changes = 0;

        loop {
            let change_list = self
                .provider
                .get_changes(Some(&cursor))
                .await
                .context("Failed to get changes")?;

            for change in &change_list.changes {
                self.apply_change(change)?;
                total_changes += 1;
            }

            cursor = change_list.cursor.clone();

            self.db.with_conn(|conn| {
                cloudsync_db::sync_cursors::upsert_cursor(
                    conn,
                    &self.account_id.to_string(),
                    &cursor,
                )
            })?;

            if !change_list.has_more {
                break;
            }
        }

        info!(
            "Incremental sync complete: {} changes applied",
            total_changes
        );
        Ok(total_changes)
    }

    /// Applies a single change from the provider's change feed.
    fn apply_change(&self, change: &Change) -> Result<()> {
        if change.deleted {
            self.db.with_conn(|conn| {
                let existing =
                    cloudsync_db::get_file_by_provider_id(conn, &self.account_id, &change.file_id)?;
                if let Some(file) = existing {
                    cloudsync_db::delete_file(conn, file.id)?;
                }
                Ok(())
            })?;
            return Ok(());
        }

        if let Some(item) = &change.item {
            self.upsert_cloud_item(item)?;
        }

        Ok(())
    }

    /// Inserts or updates a cloud item in the database.
    fn upsert_cloud_item(&self, item: &CloudItem) -> Result<()> {
        self.db.with_conn(|conn| {
            let existing = cloudsync_db::get_file_by_provider_id(conn, &self.account_id, &item.id)?;

            match existing {
                Some(mut db_file) => {
                    db_file.name = item.name.clone();
                    db_file.path = item.path.clone();
                    db_file.size = item.size;
                    db_file.content_hash = item.content_hash.clone();
                    db_file.is_folder = item.is_folder;
                    db_file.modified_at = item.modified;
                    cloudsync_db::update_file(conn, &db_file)?;
                }
                None => {
                    let db_file = cloud_item_to_db_file(&self.account_id, item);
                    cloudsync_db::create_file(conn, &db_file)?;
                }
            }

            Ok(())
        })?;
        Ok(())
    }
}

/// Converts a `CloudItem` to a database `File` record.
///
/// Sets the state to `CloudOnly` since no content has been downloaded.
pub fn cloud_item_to_db_file(account_id: &AccountId, item: &CloudItem) -> File {
    let mut file = File::new(
        account_id.clone(),
        item.id.clone(),
        item.path.clone(),
        item.name.clone(),
        item.size,
        item.content_hash.clone(),
        item.is_folder,
        item.modified,
    );
    file.state = FileState::CloudOnly;
    file
}

/// Handle for a running background sync thread.
///
/// Stops the sync thread and joins it when dropped.
pub struct SyncHandle {
    stop_flag: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SyncHandle {
    /// Signals the background thread to stop.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

impl Drop for SyncHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts a background thread that runs initial sync then periodic incremental syncs.
///
/// The thread creates its own tokio runtime since FUSE callbacks are synchronous.
pub fn start_background_sync(
    provider: Arc<dyn CloudProvider>,
    db: Database,
    account_id: AccountId,
    sync_interval: Duration,
) -> SyncHandle {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let thread = std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                error!("Failed to create tokio runtime for sync thread: {}", e);
                return;
            }
        };

        let syncer = MetadataSyncer::new(provider, db, account_id);

        // Run initial sync
        if let Err(e) = rt.block_on(syncer.initial_sync()) {
            error!("Initial sync failed: {}", e);
        }

        // Periodic incremental sync
        while !stop_flag_clone.load(Ordering::Relaxed) {
            // Sleep in small increments to allow responsive shutdown
            let sleep_end = std::time::Instant::now() + sync_interval;
            while std::time::Instant::now() < sleep_end {
                if stop_flag_clone.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }

            if stop_flag_clone.load(Ordering::Relaxed) {
                break;
            }

            if let Err(e) = rt.block_on(syncer.incremental_sync()) {
                error!("Incremental sync failed: {}", e);
            }
        }

        info!("Background sync thread stopped");
    });

    SyncHandle {
        stop_flag,
        thread: Some(thread),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use cloudsync_core::types::{ChangeList, FileId, FileVersion, ShareOptions, TransferProgress};
    use cloudsync_db::{
        accounts, Migration, ACCOUNTS_MIGRATION, FILES_MIGRATION, SYNC_CURSORS_MIGRATION,
        VFS_INODES_MIGRATION,
    };
    use std::path::Path;
    use std::sync::Mutex;
    use url::Url;

    /// Mock cloud provider for testing sync logic.
    struct MockProvider {
        items: Mutex<Vec<CloudItem>>,
        changes: Mutex<Option<ChangeList>>,
    }

    impl MockProvider {
        fn new(items: Vec<CloudItem>, changes: Option<ChangeList>) -> Self {
            Self {
                items: Mutex::new(items),
                changes: Mutex::new(changes),
            }
        }

        fn set_changes(&self, changes: ChangeList) {
            *self.changes.lock().unwrap() = Some(changes);
        }
    }

    #[async_trait]
    impl CloudProvider for MockProvider {
        fn id(&self) -> &'static str {
            "mock"
        }

        fn display_name(&self) -> &'static str {
            "Mock Provider"
        }

        async fn authenticate(&mut self, _auth_code: &str) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }

        async fn refresh_token(&mut self) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }

        async fn list_folder(
            &self,
            _path: &CloudPath,
        ) -> cloudsync_core::error::Result<Vec<CloudItem>> {
            Ok(self.items.lock().unwrap().clone())
        }

        async fn download(
            &self,
            _id: &FileId,
            _dest: &Path,
            _progress: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        ) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }

        async fn upload(
            &self,
            _src: &Path,
            _dest: &CloudPath,
            _progress: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        ) -> cloudsync_core::error::Result<CloudItem> {
            unimplemented!()
        }

        async fn delete(&self, _id: &FileId) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }

        async fn move_item(
            &self,
            _id: &FileId,
            _new_path: &CloudPath,
        ) -> cloudsync_core::error::Result<()> {
            unimplemented!()
        }

        async fn get_changes(
            &self,
            _cursor: Option<&str>,
        ) -> cloudsync_core::error::Result<ChangeList> {
            let changes = self.changes.lock().unwrap();
            Ok(changes.clone().unwrap_or_else(|| ChangeList {
                changes: vec![],
                cursor: "initial_cursor".to_string(),
                has_more: false,
            }))
        }

        async fn create_share_link(
            &self,
            _id: &FileId,
            _options: ShareOptions,
        ) -> cloudsync_core::error::Result<Url> {
            unimplemented!()
        }

        async fn get_metadata(&self, _id: &FileId) -> cloudsync_core::error::Result<CloudItem> {
            unimplemented!()
        }

        async fn get_versions(
            &self,
            _id: &FileId,
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
                    cloudsync_core::types::ProviderId::GoogleDrive,
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

    fn make_test_items() -> Vec<CloudItem> {
        let now = Utc::now();
        vec![
            CloudItem::file(
                FileId::new("file1"),
                "document.pdf".to_string(),
                CloudPath::new("/document.pdf"),
                1024,
                now,
            ),
            CloudItem::file(
                FileId::new("file2"),
                "photo.jpg".to_string(),
                CloudPath::new("/photo.jpg"),
                2048,
                now,
            ),
            CloudItem::folder(
                FileId::new("folder1"),
                "Documents".to_string(),
                CloudPath::new("/Documents"),
                now,
            ),
        ]
    }

    #[test]
    fn test_cloud_item_to_db_file_conversion() {
        let account_id = AccountId::new();
        let now = Utc::now();

        let item = CloudItem::file(
            FileId::new("f1"),
            "test.txt".to_string(),
            CloudPath::new("/test.txt"),
            512,
            now,
        );

        let db_file = cloud_item_to_db_file(&account_id, &item);

        assert_eq!(db_file.account_id, account_id);
        assert_eq!(db_file.provider_file_id, FileId::new("f1"));
        assert_eq!(db_file.name, "test.txt");
        assert_eq!(db_file.path, CloudPath::new("/test.txt"));
        assert_eq!(db_file.size, Some(512));
        assert!(!db_file.is_folder);
        assert_eq!(db_file.state, FileState::CloudOnly);
    }

    #[test]
    fn test_cloud_item_folder_to_db_file() {
        let account_id = AccountId::new();
        let now = Utc::now();

        let item = CloudItem::folder(
            FileId::new("d1"),
            "Photos".to_string(),
            CloudPath::new("/Photos"),
            now,
        );

        let db_file = cloud_item_to_db_file(&account_id, &item);

        assert!(db_file.is_folder);
        assert_eq!(db_file.size, None);
        assert_eq!(db_file.state, FileState::CloudOnly);
    }

    #[tokio::test]
    async fn test_initial_sync_populates_files_table() {
        let (db, account_id) = setup_test_db();
        let items = make_test_items();
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider, db.clone(), account_id.clone());
        let count = syncer.initial_sync().await.unwrap();

        assert_eq!(count, 3);

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();
        assert_eq!(files.len(), 3);
    }

    #[tokio::test]
    async fn test_initial_sync_stores_cursor() {
        let (db, account_id) = setup_test_db();
        let provider = Arc::new(MockProvider::new(vec![], None));

        let syncer = MetadataSyncer::new(provider, db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        let cursor = db
            .with_conn(|conn| cloudsync_db::sync_cursors::get_cursor(conn, &account_id.to_string()))
            .unwrap();
        assert_eq!(cursor, Some("initial_cursor".to_string()));
    }

    #[tokio::test]
    async fn test_initial_sync_is_idempotent() {
        let (db, account_id) = setup_test_db();
        let items = make_test_items();
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider, db.clone(), account_id.clone());

        // Run twice
        syncer.initial_sync().await.unwrap();
        syncer.initial_sync().await.unwrap();

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();
        // Should still be 3, not 6
        assert_eq!(files.len(), 3);
    }

    #[tokio::test]
    async fn test_initial_sync_handles_folders_and_files() {
        let (db, account_id) = setup_test_db();
        let items = make_test_items();
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider, db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();

        let folders: Vec<_> = files.iter().filter(|f| f.is_folder).collect();
        let regular_files: Vec<_> = files.iter().filter(|f| !f.is_folder).collect();

        assert_eq!(folders.len(), 1);
        assert_eq!(regular_files.len(), 2);
        assert_eq!(folders[0].name, "Documents");
    }

    #[tokio::test]
    async fn test_incremental_sync_creates_new_files() {
        let (db, account_id) = setup_test_db();
        let provider = Arc::new(MockProvider::new(vec![], None));

        let syncer = MetadataSyncer::new(provider.clone(), db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        // Set up a change that adds a new file
        let new_item = CloudItem::file(
            FileId::new("new_file"),
            "new.txt".to_string(),
            CloudPath::new("/new.txt"),
            100,
            Utc::now(),
        );

        provider.set_changes(ChangeList {
            changes: vec![Change {
                item: Some(new_item),
                file_id: FileId::new("new_file"),
                deleted: false,
                timestamp: Utc::now(),
            }],
            cursor: "cursor_v2".to_string(),
            has_more: false,
        });

        let changes = syncer.incremental_sync().await.unwrap();
        assert_eq!(changes, 1);

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "new.txt");
    }

    #[tokio::test]
    async fn test_incremental_sync_updates_modified_files() {
        let (db, account_id) = setup_test_db();
        let items = vec![CloudItem::file(
            FileId::new("file1"),
            "original.txt".to_string(),
            CloudPath::new("/original.txt"),
            100,
            Utc::now(),
        )];
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider.clone(), db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        // Change: file renamed and resized
        let updated_item = CloudItem::file(
            FileId::new("file1"),
            "renamed.txt".to_string(),
            CloudPath::new("/renamed.txt"),
            200,
            Utc::now(),
        );

        provider.set_changes(ChangeList {
            changes: vec![Change {
                item: Some(updated_item),
                file_id: FileId::new("file1"),
                deleted: false,
                timestamp: Utc::now(),
            }],
            cursor: "cursor_v2".to_string(),
            has_more: false,
        });

        syncer.incremental_sync().await.unwrap();

        let file = db
            .with_conn(|conn| {
                cloudsync_db::get_file_by_provider_id(conn, &account_id, &FileId::new("file1"))
            })
            .unwrap()
            .unwrap();

        assert_eq!(file.name, "renamed.txt");
        assert_eq!(file.size, Some(200));
    }

    #[tokio::test]
    async fn test_incremental_sync_deletes_removed_files() {
        let (db, account_id) = setup_test_db();
        let items = vec![CloudItem::file(
            FileId::new("file1"),
            "to_delete.txt".to_string(),
            CloudPath::new("/to_delete.txt"),
            100,
            Utc::now(),
        )];
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider.clone(), db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        // Change: file deleted
        provider.set_changes(ChangeList {
            changes: vec![Change {
                item: None,
                file_id: FileId::new("file1"),
                deleted: true,
                timestamp: Utc::now(),
            }],
            cursor: "cursor_v2".to_string(),
            has_more: false,
        });

        syncer.incremental_sync().await.unwrap();

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_incremental_sync_updates_cursor() {
        let (db, account_id) = setup_test_db();
        let provider = Arc::new(MockProvider::new(vec![], None));

        let syncer = MetadataSyncer::new(provider.clone(), db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        provider.set_changes(ChangeList {
            changes: vec![],
            cursor: "updated_cursor".to_string(),
            has_more: false,
        });

        syncer.incremental_sync().await.unwrap();

        let cursor = db
            .with_conn(|conn| cloudsync_db::sync_cursors::get_cursor(conn, &account_id.to_string()))
            .unwrap();
        assert_eq!(cursor, Some("updated_cursor".to_string()));
    }

    #[tokio::test]
    async fn test_incremental_sync_without_cursor_falls_back_to_initial() {
        let (db, account_id) = setup_test_db();
        let items = make_test_items();
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider, db.clone(), account_id.clone());

        // No initial_sync() called — no cursor stored
        let count = syncer.incremental_sync().await.unwrap();
        assert_eq!(count, 3);

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();
        assert_eq!(files.len(), 3);
    }

    #[tokio::test]
    async fn test_all_synced_files_have_cloud_only_state() {
        let (db, account_id) = setup_test_db();
        let items = make_test_items();
        let provider = Arc::new(MockProvider::new(items, None));

        let syncer = MetadataSyncer::new(provider, db.clone(), account_id.clone());
        syncer.initial_sync().await.unwrap();

        let files = db
            .with_conn(|conn| cloudsync_db::list_files(conn, &account_id, None))
            .unwrap();

        for file in &files {
            assert_eq!(file.state, FileState::CloudOnly);
        }
    }
}
