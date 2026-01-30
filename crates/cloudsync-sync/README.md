# CloudSync Sync Engine

This crate provides the sync orchestration layer for CloudSync, including a priority queue for managing sync operations (upload, download, delete).

## Features

- **Priority-based Queue**: Operations are processed based on priority (High > Normal > Low)
- **FIFO within Priority**: Operations with the same priority are processed in first-in-first-out order
- **Thread-safe**: Built with `Arc<Mutex<>>` for safe concurrent access
- **Operation Types**: Support for Upload, Download, and Delete operations
- **Async/Await**: Fully asynchronous API using Tokio

## Usage

Add this crate as a dependency:

```toml
[dependencies]
cloudsync-sync = { path = "crates/cloudsync-sync" }
```

### Basic Example

```rust
use cloudsync_sync::{SyncQueue, SyncOperation, SyncPriority};
use cloudsync_core::types::{AccountId, CloudPath, FileId};

#[tokio::main]
async fn main() {
    let queue = SyncQueue::new();
    let account_id = AccountId::new();

    // Add a high-priority download (user-initiated)
    queue.push(
        SyncOperation::download_high_priority(
            account_id.clone(),
            FileId::new("file123"),
            CloudPath::new("/documents/important.pdf"),
            Some(1024),
        )
    ).await;

    // Add a normal-priority background download
    queue.push(
        SyncOperation::download_normal_priority(
            account_id,
            FileId::new("file456"),
            CloudPath::new("/photos/vacation.jpg"),
            Some(2048),
        )
    ).await;

    // Process operations in priority order
    while let Ok(operation) = queue.pop().await {
        println!("Processing: {} {}", operation.operation_type, operation.path);
        // Handle the operation...
    }
}
```

## Priority Levels

- **High**: User-initiated operations (e.g., "download now" from context menu)
- **Normal**: Regular sync operations discovered by change detection
- **Low**: Background operations, deferred tasks

## API Reference

### SyncQueue

Main queue structure for managing sync operations.

#### Methods

- `new()` - Create a new empty queue
- `push(operation)` - Add an operation to the queue
- `pop()` - Remove and return the highest priority operation
- `peek()` - View the next operation without removing it
- `remove(id)` - Remove a specific operation by ID
- `len()` - Get the number of queued operations
- `is_empty()` - Check if the queue is empty
- `list()` - Get all operations in priority order
- `list_by_priority(priority)` - Get operations filtered by priority
- `clear()` - Remove all operations

### SyncOperation

Represents a single sync operation.

#### Constructors

- `new()` - Create a custom operation
- `upload_high_priority()` - High-priority upload
- `download_normal_priority()` - Normal-priority download
- `download_high_priority()` - High-priority download
- `delete()` - Delete operation with specified priority

## Testing

Run the test suite:

```bash
cargo test -p cloudsync-sync
```

Run the example:

```bash
cargo run --example queue_usage
```

## Architecture

The sync queue uses a `BinaryHeap` internally for O(log n) insertions and O(log n) removals of the highest priority item. Operations are ordered by:

1. Priority (High > Normal > Low)
2. Creation time (older first, within same priority)

This ensures that user-initiated actions are always processed before background sync operations, while maintaining fairness within each priority level through FIFO ordering.
