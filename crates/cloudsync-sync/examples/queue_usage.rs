//! Example demonstrating the sync queue usage.
//!
//! Run with: cargo run --example queue_usage

use cloudsync_core::types::{AccountId, CloudPath, FileId};
use cloudsync_sync::{SyncOperation, SyncOperationType, SyncPriority, SyncQueue};

#[tokio::main]
async fn main() {
    println!("=== CloudSync Priority Queue Demo ===\n");

    let queue = SyncQueue::new();
    let account_id = AccountId::new();

    // Simulate various sync operations
    println!("Adding operations to queue...");

    // Background sync (low priority)
    queue
        .push(SyncOperation::new(
            account_id.clone(),
            FileId::new("bg_file_1"),
            CloudPath::new("/background/file1.txt"),
            SyncOperationType::Download,
            SyncPriority::Low,
            Some(1024),
        ))
        .await;
    println!("  ✓ Added low priority background download");

    // Normal sync operation
    queue
        .push(SyncOperation::download_normal_priority(
            account_id.clone(),
            FileId::new("normal_file"),
            CloudPath::new("/documents/report.pdf"),
            Some(5120),
        ))
        .await;
    println!("  ✓ Added normal priority download");

    // User-initiated operation (high priority)
    queue
        .push(SyncOperation::download_high_priority(
            account_id.clone(),
            FileId::new("urgent_file"),
            CloudPath::new("/important/presentation.pptx"),
            Some(10240),
        ))
        .await;
    println!("  ✓ Added high priority download (user-initiated)");

    // Another background upload
    queue
        .push(SyncOperation::upload_high_priority(
            account_id.clone(),
            FileId::new("user_upload"),
            CloudPath::new("/photos/vacation.jpg"),
            Some(2048),
        ))
        .await;
    println!("  ✓ Added high priority upload");

    println!("\nQueue status:");
    println!("  Total operations: {}", queue.len().await);
    println!("  Is empty: {}\n", queue.is_empty().await);

    // List operations by priority
    println!("High priority operations:");
    let high_ops = queue.list_by_priority(SyncPriority::High).await;
    for op in &high_ops {
        println!(
            "  - {} {} ({})",
            op.operation_type,
            op.path.as_str(),
            op.file_id
        );
    }

    println!("\nNormal priority operations:");
    let normal_ops = queue.list_by_priority(SyncPriority::Normal).await;
    for op in &normal_ops {
        println!(
            "  - {} {} ({})",
            op.operation_type,
            op.path.as_str(),
            op.file_id
        );
    }

    println!("\nLow priority operations:");
    let low_ops = queue.list_by_priority(SyncPriority::Low).await;
    for op in &low_ops {
        println!(
            "  - {} {} ({})",
            op.operation_type,
            op.path.as_str(),
            op.file_id
        );
    }

    // Process operations in priority order
    println!("\n=== Processing queue in priority order ===\n");

    let mut count = 1;
    while let Ok(operation) = queue.pop().await {
        println!(
            "{}. [{:>6}] {} {} - {}",
            count,
            operation.priority.to_string().to_uppercase(),
            operation.operation_type.to_string().to_uppercase(),
            operation.path.as_str(),
            operation.file_id
        );
        count += 1;
    }

    println!("\n✓ All operations processed!");
    println!("Queue is empty: {}", queue.is_empty().await);
}
