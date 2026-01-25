//! Manual testing demo for account CRUD operations.
//!
//! Run with: cargo run --example account_crud_demo

use chrono::{Duration, Utc};
use cloudsync_core::types::ProviderId;
use cloudsync_db::{
    create_account, delete_account, get_account_by_id, list_accounts, update_account, Account,
    Database, Migration, ACCOUNTS_MIGRATION,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== CloudSync Account CRUD Demo ===\n");

    // Create a file-based database for testing
    let db_path = "test_accounts.db";

    // Clean up old database if it exists
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(format!("{}-shm", db_path));
    let _ = std::fs::remove_file(format!("{}-wal", db_path));

    println!("Creating database at: {}", db_path);

    let migrations = vec![Migration {
        version: 1,
        description: "Create accounts table",
        sql: ACCOUNTS_MIGRATION,
    }];

    let db = Database::open(db_path, migrations)?;
    println!("✓ Database created and migrations applied\n");

    // CREATE: Add some test accounts
    println!("--- CREATE Operations ---");

    let account1 = Account::new(
        ProviderId::GoogleDrive,
        "alice@example.com".to_string(),
        "gdrive_access_token_123".to_string(),
        Some("gdrive_refresh_token_456".to_string()),
        Some(Utc::now() + Duration::hours(1)),
    );

    let account2 = Account::new(
        ProviderId::Dropbox,
        "bob@example.com".to_string(),
        "dropbox_token_789".to_string(),
        Some("dropbox_refresh_abc".to_string()),
        None,
    );

    let account3 = Account::new(
        ProviderId::OneDrive,
        "charlie@example.com".to_string(),
        "onedrive_token_xyz".to_string(),
        None,
        Some(Utc::now() + Duration::hours(2)),
    );

    let account1_id = db.with_conn(|conn| create_account(conn, &account1))?;
    println!("✓ Created account 1: {} ({})", account1.email, account1_id);

    let account2_id = db.with_conn(|conn| create_account(conn, &account2))?;
    println!("✓ Created account 2: {} ({})", account2.email, account2_id);

    let account3_id = db.with_conn(|conn| create_account(conn, &account3))?;
    println!("✓ Created account 3: {} ({})", account3.email, account3_id);

    // Test duplicate detection
    println!("\nTesting duplicate detection...");
    let duplicate_result = db.with_conn(|conn| {
        let duplicate = Account::new(
            ProviderId::GoogleDrive,
            "alice@example.com".to_string(),
            "different_token".to_string(),
            None,
            None,
        );
        create_account(conn, &duplicate)
    });

    match duplicate_result {
        Err(e) => println!("✓ Duplicate correctly rejected: {}", e),
        Ok(_) => println!("✗ Duplicate was not rejected (this shouldn't happen!)"),
    }

    // READ: List all accounts
    println!("\n--- READ Operations ---");

    let all_accounts = db.with_conn(|conn| list_accounts(conn, false))?;
    println!("Total accounts: {}", all_accounts.len());
    for (i, account) in all_accounts.iter().enumerate() {
        println!(
            "  {}. {} - {} ({})",
            i + 1,
            account.provider,
            account.email,
            if account.is_active {
                "active"
            } else {
                "inactive"
            }
        );
        let token_preview = if account.access_token.len() > 20 {
            format!("{}...", &account.access_token[..20])
        } else {
            account.access_token.clone()
        };
        println!("     Access token: {}", token_preview);
        println!(
            "     Refresh token: {}",
            account
                .refresh_token
                .as_ref()
                .map(|t| if t.len() > 20 {
                    format!("{}...", &t[..20])
                } else {
                    t.clone()
                })
                .unwrap_or("None".to_string())
        );
        println!(
            "     Expires: {}",
            account
                .token_expires_at
                .map(|t| t.to_rfc3339())
                .unwrap_or("Never".to_string())
        );
    }

    // Read single account
    println!("\nFetching account by ID: {}", account1_id);
    let retrieved = db.with_conn(|conn| get_account_by_id(conn, &account1_id))?;
    match retrieved {
        Some(acc) => {
            println!("✓ Found account:");
            println!("  Provider: {}", acc.provider);
            println!("  Email: {}", acc.email);
            println!("  Created: {}", acc.created_at.to_rfc3339());
        }
        None => println!("✗ Account not found"),
    }

    // UPDATE: Modify an account
    println!("\n--- UPDATE Operations ---");

    let mut account_to_update = db
        .with_conn(|conn| get_account_by_id(conn, &account2_id))?
        .unwrap();

    println!("Original email: {}", account_to_update.email);
    println!("Original status: active={}", account_to_update.is_active);

    account_to_update.email = "bob_updated@example.com".to_string();
    account_to_update.access_token = "new_dropbox_token_999".to_string();
    account_to_update.is_active = false;

    let updated = db.with_conn(|conn| update_account(conn, &account_to_update))?;
    println!("✓ Account updated: {}", updated);

    let updated_account = db
        .with_conn(|conn| get_account_by_id(conn, &account2_id))?
        .unwrap();
    println!("New email: {}", updated_account.email);
    println!("New status: active={}", updated_account.is_active);
    println!(
        "Updated timestamp changed: {}",
        updated_account.updated_at > account_to_update.created_at
    );

    // List only active accounts
    println!("\nListing active accounts only:");
    let active_accounts = db.with_conn(|conn| list_accounts(conn, true))?;
    println!("Active accounts: {}", active_accounts.len());
    for account in &active_accounts {
        println!("  - {} ({})", account.email, account.provider);
    }

    // DELETE: Remove an account
    println!("\n--- DELETE Operations ---");

    println!("Deleting account: {}", account3_id);
    let deleted = db.with_conn(|conn| delete_account(conn, &account3_id))?;
    println!("✓ Account deleted: {}", deleted);

    let deleted_check = db.with_conn(|conn| get_account_by_id(conn, &account3_id))?;
    println!(
        "✓ Verification - account exists: {}",
        deleted_check.is_some()
    );

    // Final count
    println!("\n--- Final State ---");
    let final_accounts = db.with_conn(|conn| list_accounts(conn, false))?;
    println!("Remaining accounts: {}", final_accounts.len());
    for account in &final_accounts {
        println!(
            "  - {} ({}) - {}",
            account.email,
            account.provider,
            if account.is_active {
                "active"
            } else {
                "inactive"
            }
        );
    }

    println!("\n=== Demo Complete ===");
    println!("\nTo inspect the database directly, run:");
    println!("  sqlite3 {}", db_path);
    println!("  sqlite> SELECT id, provider, email, is_active FROM accounts;");
    println!("\nTo clean up:");
    println!("  rm {}", db_path);

    Ok(())
}
