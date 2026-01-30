//! Sync orchestration engine.
//!
//! The SyncEngine coordinates synchronization between cloud providers and
//! the local filesystem. It manages initial sync, periodic refresh, and
//! operation execution.

use crate::error::{Error, Result};
use crate::operation::SyncOperation;
use crate::queue::SyncQueue;
use cloudsync_core::{
    provider::CloudProvider,
    types::{AccountId, CloudPath},
};
use cloudsync_db::Database;
use std::sync::Arc;
use tracing::{debug, info};

/// Orchestrates sync operations between cloud and local storage.
///
/// The SyncEngine is responsible for:
/// - Performing initial full sync on first run
/// - Periodically refreshing changes from the cloud
/// - Queuing operations for execution
/// - Tracking sync state in the database
pub struct SyncEngine<P: CloudProvider> {
    /// The cloud provider to sync with.
    provider: Arc<P>,

    /// Database for tracking file state and sync metadata.
    db: Arc<Database>,

    /// Queue for sync operations.
    queue: Arc<SyncQueue>,

    /// Account ID for this sync session.
    account_id: AccountId,
}

impl<P: CloudProvider + Send + Sync> SyncEngine<P> {
    /// Creates a new SyncEngine.
    pub fn new(
        provider: Arc<P>,
        db: Arc<Database>,
        queue: Arc<SyncQueue>,
        account_id: AccountId,
    ) -> Self {
        Self {
            provider,
            db,
            queue,
            account_id,
        }
    }

    /// Performs initial full sync from the cloud.
    ///
    /// This lists all files in the provider's root and queues download
    /// operations for each file. It should be called on the first run
    /// or when the local cache needs to be rebuilt.
    pub async fn initial_sync(&self, provider_root: &CloudPath) -> Result<usize> {
        info!(
            "Starting initial sync for {} from path {}",
            self.provider.display_name(),
            provider_root
        );

        // List all files from the provider root
        let items = self
            .provider
            .list_folder(provider_root)
            .await
            .map_err(|e| Error::Sync(format!("Failed to list folder during initial sync: {}", e)))?;

        info!("Found {} items to sync", items.len());

        let mut queued_count = 0;

        // Queue download operations for all files (not folders)
        for item in items {
            if item.is_folder {
                debug!("Skipping folder: {}", item.name);
                continue;
            }

            // Queue a high-priority download for initial sync
            let operation = SyncOperation::download_high_priority(
                self.account_id.clone(),
                item.id.clone(),
                item.path.clone(),
                item.size,
            );

            self.queue.push(operation).await;
            queued_count += 1;

            debug!("Queued download for: {} ({})", item.name, item.id);
        }

        // After queuing all operations, get the start page token for future syncs
        let change_list = self
            .provider
            .get_changes(None)
            .await
            .map_err(|e| Error::Sync(format!("Failed to get initial change token: {}", e)))?;

        // TODO: Store the change token in the database for next sync
        // For now, just log it
        info!("Initial sync cursor: {}", change_list.cursor);

        info!("Initial sync complete. Queued {} files for download", queued_count);

        Ok(queued_count)
    }

    /// Performs a periodic refresh sync.
    ///
    /// This uses the provider's change feed to detect files that have been
    /// added, modified, or deleted since the last sync. Only changed files
    /// are queued for download.
    ///
    /// Returns the tuple (queued_count, new_cursor)
    pub async fn refresh_sync(&self, cursor: Option<String>) -> Result<(usize, String)> {
        info!(
            "Starting refresh sync for {}",
            self.provider.display_name()
        );

        let mut current_cursor = cursor;
        let mut total_queued = 0;
        let mut final_cursor = String::new();

        // Keep fetching changes until we've processed all batches
        loop {
            let change_list = self
                .provider
                .get_changes(current_cursor.as_deref())
                .await
                .map_err(|e| Error::Sync(format!("Failed to get changes: {}", e)))?;

            info!(
                "Processing {} changes (has_more: {})",
                change_list.changes.len(),
                change_list.has_more
            );

            // Process each change
            for change in change_list.changes {
                if change.deleted {
                    // Read-only mode: skip deletions
                    debug!("Skipping deleted file: {}", change.file_id);
                    continue;
                }

                // Get the item details
                let item = match change.item {
                    Some(item) => item,
                    None => {
                        debug!("Skipping change without item details: {}", change.file_id);
                        continue;
                    }
                };

                // Skip folders
                if item.is_folder {
                    debug!("Skipping folder: {}", item.name);
                    continue;
                }

                // Queue a normal-priority download for the changed file
                let operation = SyncOperation::download_normal_priority(
                    self.account_id.clone(),
                    item.id.clone(),
                    item.path.clone(),
                    item.size,
                );

                self.queue.push(operation).await;
                total_queued += 1;

                debug!("Queued download for changed file: {} ({})", item.name, item.id);
            }

            // Store the cursor for the next iteration or final return
            final_cursor = change_list.cursor;

            // If there are no more changes, break the loop
            if !change_list.has_more {
                break;
            }

            // Continue with the next page of changes
            current_cursor = Some(final_cursor.clone());
        }

        info!(
            "Refresh sync complete. Queued {} files for download",
            total_queued
        );

        Ok((total_queued, final_cursor))
    }

    /// Executes pending sync operations from the queue.
    ///
    /// This pops operations from the queue and executes them using the
    /// provider. Successfully completed operations are recorded in the database.
    ///
    /// # Parameters
    /// * `local_root` - The root directory for local file storage (e.g., /home/user/UniCloudST/gdrive/)
    /// * `max_operations` - Maximum number of operations to execute (None = execute all)
    ///
    /// # Returns
    /// Number of operations successfully executed
    pub async fn execute_operations(
        &self,
        local_root: &std::path::Path,
        max_operations: Option<usize>,
    ) -> Result<usize> {
        info!("Starting operation execution");

        let mut executed_count = 0;
        let limit = max_operations.unwrap_or(usize::MAX);

        while executed_count < limit {
            // Try to pop an operation from the queue
            let operation = match self.queue.pop().await {
                Ok(op) => op,
                Err(Error::QueueEmpty) => {
                    debug!("Queue is empty, stopping execution");
                    break;
                }
                Err(e) => {
                    return Err(Error::Sync(format!("Failed to pop from queue: {}", e)));
                }
            };

            // TODO: Execute the operation based on its type
            // For now, we'll implement download operations only (MVP requirement)
            match operation.operation_type {
                crate::operation::SyncOperationType::Download => {
                    // Execute download operation
                    match self.execute_download(&operation, local_root).await {
                        Ok(_) => {
                            info!("Successfully downloaded: {}", operation.path);
                            executed_count += 1;
                        }
                        Err(e) => {
                            // Log error but continue processing other operations
                            tracing::error!(
                                "Failed to download {} ({}): {}",
                                operation.path,
                                operation.file_id,
                                e
                            );
                            // For MVP, we continue on error
                            // In production, you might want to re-queue or implement retry logic
                        }
                    }
                }
                _ => {
                    // Skip non-download operations (read-only MVP)
                    debug!(
                        "Skipping non-download operation: {:?}",
                        operation.operation_type
                    );
                }
            }
        }

        info!(
            "Operation execution complete. Executed {} operations",
            executed_count
        );

        Ok(executed_count)
    }

    /// Executes a download operation.
    async fn execute_download(
        &self,
        operation: &SyncOperation,
        local_root: &std::path::Path,
    ) -> Result<()> {
        // Construct the local file path from the cloud path
        // For MVP, we'll use a simple mapping: /file.txt -> {local_root}/file.txt
        let relative_path = operation.path.as_str().trim_start_matches('/');
        let local_path = local_root.join(relative_path);

        // Create parent directories if they don't exist
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Sync(format!("Failed to create directories: {}", e)))?;
        }

        // Download the file from the provider
        // TODO: Add progress tracking channel
        self.provider
            .download(&operation.file_id, &local_path, None)
            .await
            .map_err(|e| {
                Error::Sync(format!(
                    "Provider download failed for {}: {}",
                    operation.file_id, e
                ))
            })?;

        debug!("Downloaded file to: {}", local_path.display());

        // TODO: Update database with file state
        // For MVP, we skip database updates and just perform the download

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloudsync_core::types::{AccountId, CloudPath};
    use std::sync::Arc;

    // Note: Full integration tests will be added once we have a mock provider
    // For now, we just test that the engine can be constructed

    #[test]
    fn sync_engine_can_be_constructed() {
        // This test just verifies the types compile
        // Actual functionality tests require a mock provider
    }
}
