//! Priority queue for sync operations.

use crate::error::{Error, Result};
use crate::operation::{OperationId, SyncOperation, SyncPriority};
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Wrapper for operations in the priority queue.
///
/// Operations are ordered by:
/// 1. Priority (High > Normal > Low)
/// 2. Creation time (older operations first within same priority)
#[derive(Debug, Clone)]
struct QueuedOperation {
    operation: SyncOperation,
}

impl PartialEq for QueuedOperation {
    fn eq(&self, other: &Self) -> bool {
        self.operation.id == other.operation.id
    }
}

impl Eq for QueuedOperation {}

impl PartialOrd for QueuedOperation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedOperation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First compare by priority (higher priority first)
        // self > other if self has higher priority
        match self.operation.priority.cmp(&other.operation.priority) {
            std::cmp::Ordering::Equal => {
                // For same priority, older operations come first (FIFO)
                // Reverse comparison: self is "greater" if it's older
                other.operation.created_at.cmp(&self.operation.created_at)
            }
            ordering => ordering,
        }
    }
}

/// Thread-safe priority queue for sync operations.
///
/// Operations are processed in priority order (High > Normal > Low),
/// with FIFO ordering within the same priority level.
pub struct SyncQueue {
    /// The underlying priority queue.
    queue: Arc<Mutex<BinaryHeap<QueuedOperation>>>,
}

impl SyncQueue {
    /// Creates a new empty sync queue.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
        }
    }

    /// Adds a sync operation to the queue.
    ///
    /// Operations are automatically ordered by priority and creation time.
    pub async fn push(&self, operation: SyncOperation) {
        let mut queue = self.queue.lock().await;
        queue.push(QueuedOperation { operation });
    }

    /// Retrieves and removes the highest priority operation from the queue.
    ///
    /// Returns `Ok(SyncOperation)` if an operation is available,
    /// or `Err(Error::QueueEmpty)` if the queue is empty.
    pub async fn pop(&self) -> Result<SyncOperation> {
        let mut queue = self.queue.lock().await;
        queue
            .pop()
            .map(|queued| queued.operation)
            .ok_or(Error::QueueEmpty)
    }

    /// Returns the next operation without removing it from the queue.
    ///
    /// Returns `Ok(SyncOperation)` if an operation is available,
    /// or `Err(Error::QueueEmpty)` if the queue is empty.
    pub async fn peek(&self) -> Result<SyncOperation> {
        let queue = self.queue.lock().await;
        queue
            .peek()
            .map(|queued| queued.operation.clone())
            .ok_or(Error::QueueEmpty)
    }

    /// Removes a specific operation from the queue by its ID.
    ///
    /// Returns `Ok(SyncOperation)` if the operation was found and removed,
    /// or `Err(Error::OperationNotFound)` if not found.
    pub async fn remove(&self, operation_id: &OperationId) -> Result<SyncOperation> {
        let mut queue = self.queue.lock().await;

        // BinaryHeap doesn't support direct removal, so we need to rebuild it
        let mut operations: Vec<QueuedOperation> = queue.drain().collect();

        // Find and remove the operation
        let position = operations
            .iter()
            .position(|op| op.operation.id == *operation_id)
            .ok_or_else(|| Error::OperationNotFound(operation_id.to_string()))?;

        let removed = operations.remove(position);

        // Rebuild the heap without the removed operation
        *queue = operations.into_iter().collect();

        Ok(removed.operation)
    }

    /// Returns the number of operations in the queue.
    pub async fn len(&self) -> usize {
        let queue = self.queue.lock().await;
        queue.len()
    }

    /// Returns whether the queue is empty.
    pub async fn is_empty(&self) -> bool {
        let queue = self.queue.lock().await;
        queue.is_empty()
    }

    /// Returns all operations in the queue, ordered by priority.
    ///
    /// This does not remove operations from the queue.
    pub async fn list(&self) -> Vec<SyncOperation> {
        let queue = self.queue.lock().await;
        let mut operations: Vec<_> = queue.iter().map(|q| q.operation.clone()).collect();

        // Sort by priority and creation time (same as queue ordering)
        operations.sort_by(|a, b| match b.priority.cmp(&a.priority) {
            std::cmp::Ordering::Equal => a.created_at.cmp(&b.created_at),
            ordering => ordering,
        });

        operations
    }

    /// Returns operations filtered by priority.
    pub async fn list_by_priority(&self, priority: SyncPriority) -> Vec<SyncOperation> {
        let queue = self.queue.lock().await;
        queue
            .iter()
            .filter(|q| q.operation.priority == priority)
            .map(|q| q.operation.clone())
            .collect()
    }

    /// Clears all operations from the queue.
    pub async fn clear(&self) {
        let mut queue = self.queue.lock().await;
        queue.clear();
    }
}

impl Default for SyncQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloudsync_core::types::{AccountId, CloudPath, FileId};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn queue_starts_empty() {
        let queue = SyncQueue::new();
        assert!(queue.is_empty().await);
        assert_eq!(queue.len().await, 0);
    }

    #[tokio::test]
    async fn push_and_pop_single_operation() {
        let queue = SyncQueue::new();

        let op = SyncOperation::download_high_priority(
            AccountId::new(),
            FileId::new("file1"),
            CloudPath::new("/test.pdf"),
            Some(1024),
        );

        let op_id = op.id.clone();
        queue.push(op).await;

        assert!(!queue.is_empty().await);
        assert_eq!(queue.len().await, 1);

        let popped = queue.pop().await.unwrap();
        assert_eq!(popped.id, op_id);
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn pop_empty_queue_returns_error() {
        let queue = SyncQueue::new();
        let result = queue.pop().await;
        assert!(matches!(result, Err(Error::QueueEmpty)));
    }

    #[tokio::test]
    async fn peek_does_not_remove_operation() {
        let queue = SyncQueue::new();

        let op = SyncOperation::download_normal_priority(
            AccountId::new(),
            FileId::new("file1"),
            CloudPath::new("/test.pdf"),
            Some(1024),
        );

        let op_id = op.id.clone();
        queue.push(op).await;

        let peeked = queue.peek().await.unwrap();
        assert_eq!(peeked.id, op_id);
        assert_eq!(queue.len().await, 1);

        let popped = queue.pop().await.unwrap();
        assert_eq!(popped.id, op_id);
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn peek_empty_queue_returns_error() {
        let queue = SyncQueue::new();
        let result = queue.peek().await;
        assert!(matches!(result, Err(Error::QueueEmpty)));
    }

    #[tokio::test]
    async fn operations_ordered_by_priority() {
        let queue = SyncQueue::new();
        let account_id = AccountId::new();

        // Push operations in non-priority order
        let low = SyncOperation::new(
            account_id.clone(),
            FileId::new("low"),
            CloudPath::new("/low.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Low,
            None,
        );

        let high = SyncOperation::new(
            account_id.clone(),
            FileId::new("high"),
            CloudPath::new("/high.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::High,
            None,
        );

        let normal = SyncOperation::new(
            account_id.clone(),
            FileId::new("normal"),
            CloudPath::new("/normal.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Normal,
            None,
        );

        queue.push(low).await;
        queue.push(high.clone()).await;
        queue.push(normal.clone()).await;

        // Should pop in priority order: high, normal, low
        assert_eq!(queue.pop().await.unwrap().file_id, high.file_id);
        assert_eq!(queue.pop().await.unwrap().file_id, normal.file_id);
        assert_eq!(queue.pop().await.unwrap().file_id.0, "low");
    }

    #[tokio::test]
    async fn same_priority_uses_fifo_ordering() {
        let queue = SyncQueue::new();
        let account_id = AccountId::new();

        // Create operations with same priority but different times
        let op1 = SyncOperation::new(
            account_id.clone(),
            FileId::new("file1"),
            CloudPath::new("/file1.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Normal,
            None,
        );

        // Small delay to ensure different timestamps
        sleep(Duration::from_millis(10)).await;

        let op2 = SyncOperation::new(
            account_id.clone(),
            FileId::new("file2"),
            CloudPath::new("/file2.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Normal,
            None,
        );

        queue.push(op2.clone()).await;
        queue.push(op1.clone()).await;

        // Should pop in FIFO order (oldest first)
        assert_eq!(queue.pop().await.unwrap().file_id, op1.file_id);
        assert_eq!(queue.pop().await.unwrap().file_id, op2.file_id);
    }

    #[tokio::test]
    async fn remove_operation_by_id() {
        let queue = SyncQueue::new();
        let account_id = AccountId::new();

        let op1 = SyncOperation::download_normal_priority(
            account_id.clone(),
            FileId::new("file1"),
            CloudPath::new("/file1.pdf"),
            None,
        );

        let op2 = SyncOperation::download_normal_priority(
            account_id.clone(),
            FileId::new("file2"),
            CloudPath::new("/file2.pdf"),
            None,
        );

        let op2_id = op2.id.clone();

        queue.push(op1).await;
        queue.push(op2).await;

        assert_eq!(queue.len().await, 2);

        let removed = queue.remove(&op2_id).await.unwrap();
        assert_eq!(removed.id, op2_id);
        assert_eq!(queue.len().await, 1);
    }

    #[tokio::test]
    async fn remove_nonexistent_operation_returns_error() {
        let queue = SyncQueue::new();
        let nonexistent_id = OperationId::new();

        let result = queue.remove(&nonexistent_id).await;
        assert!(matches!(result, Err(Error::OperationNotFound(_))));
    }

    #[tokio::test]
    async fn list_returns_all_operations_in_priority_order() {
        let queue = SyncQueue::new();
        let account_id = AccountId::new();

        let high1 = SyncOperation::new(
            account_id.clone(),
            FileId::new("high1"),
            CloudPath::new("/high1.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::High,
            None,
        );

        let normal1 = SyncOperation::new(
            account_id.clone(),
            FileId::new("normal1"),
            CloudPath::new("/normal1.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Normal,
            None,
        );

        let low1 = SyncOperation::new(
            account_id.clone(),
            FileId::new("low1"),
            CloudPath::new("/low1.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Low,
            None,
        );

        queue.push(normal1.clone()).await;
        queue.push(low1.clone()).await;
        queue.push(high1.clone()).await;

        let operations = queue.list().await;
        assert_eq!(operations.len(), 3);

        // Should be in priority order
        assert_eq!(operations[0].file_id, high1.file_id);
        assert_eq!(operations[1].file_id, normal1.file_id);
        assert_eq!(operations[2].file_id, low1.file_id);

        // List should not remove operations
        assert_eq!(queue.len().await, 3);
    }

    #[tokio::test]
    async fn list_by_priority_filters_correctly() {
        let queue = SyncQueue::new();
        let account_id = AccountId::new();

        let high1 = SyncOperation::new(
            account_id.clone(),
            FileId::new("high1"),
            CloudPath::new("/high1.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::High,
            None,
        );

        let normal1 = SyncOperation::new(
            account_id.clone(),
            FileId::new("normal1"),
            CloudPath::new("/normal1.pdf"),
            crate::operation::SyncOperationType::Download,
            SyncPriority::Normal,
            None,
        );

        queue.push(high1.clone()).await;
        queue.push(normal1.clone()).await;

        let high_ops = queue.list_by_priority(SyncPriority::High).await;
        assert_eq!(high_ops.len(), 1);
        assert_eq!(high_ops[0].file_id, high1.file_id);

        let normal_ops = queue.list_by_priority(SyncPriority::Normal).await;
        assert_eq!(normal_ops.len(), 1);
        assert_eq!(normal_ops[0].file_id, normal1.file_id);

        let low_ops = queue.list_by_priority(SyncPriority::Low).await;
        assert_eq!(low_ops.len(), 0);
    }

    #[tokio::test]
    async fn clear_removes_all_operations() {
        let queue = SyncQueue::new();
        let account_id = AccountId::new();

        for i in 0..5 {
            queue
                .push(SyncOperation::download_normal_priority(
                    account_id.clone(),
                    FileId::new(format!("file{}", i)),
                    CloudPath::new(format!("/file{}.pdf", i)),
                    None,
                ))
                .await;
        }

        assert_eq!(queue.len().await, 5);

        queue.clear().await;

        assert_eq!(queue.len().await, 0);
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn concurrent_push_and_pop() {
        let queue = Arc::new(SyncQueue::new());
        let account_id = AccountId::new();

        // Spawn multiple tasks pushing operations
        let mut handles = vec![];
        for i in 0..10 {
            let queue_clone = Arc::clone(&queue);
            let account_id_clone = account_id.clone();
            let handle = tokio::spawn(async move {
                queue_clone
                    .push(SyncOperation::download_normal_priority(
                        account_id_clone,
                        FileId::new(format!("file{}", i)),
                        CloudPath::new(format!("/file{}.pdf", i)),
                        None,
                    ))
                    .await;
            });
            handles.push(handle);
        }

        // Wait for all pushes to complete
        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(queue.len().await, 10);

        // Pop all operations
        let mut popped_count = 0;
        while queue.pop().await.is_ok() {
            popped_count += 1;
        }

        assert_eq!(popped_count, 10);
        assert!(queue.is_empty().await);
    }
}
