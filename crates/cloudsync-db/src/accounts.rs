//! Account CRUD operations for cloud provider accounts.
//!
//! This module provides database operations for managing cloud provider accounts,
//! including their authentication credentials and metadata.

use crate::{DbError, DbResult};
use chrono::{DateTime, Utc};
use cloudsync_core::types::{AccountId, ProviderId};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

/// Database migration for the accounts table.
///
/// This creates the accounts table with fields for:
/// - Unique account identifier (UUID)
/// - Provider type (gdrive, dropbox, onedrive)
/// - User email/identifier
/// - OAuth tokens (access and refresh)
/// - Token expiration timestamp
/// - Account activation status
/// - Creation and modification timestamps
pub const ACCOUNTS_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL CHECK(provider IN ('googledrive', 'dropbox', 'onedrive')),
    email TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT,
    token_expires_at INTEGER,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(provider, email)
);

CREATE INDEX idx_accounts_provider ON accounts(provider);
CREATE INDEX idx_accounts_is_active ON accounts(is_active);
"#;

/// Represents a cloud provider account stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Unique identifier for this account.
    pub id: AccountId,

    /// The cloud provider type.
    pub provider: ProviderId,

    /// User's email or identifier for this account.
    pub email: String,

    /// OAuth access token for API requests.
    pub access_token: String,

    /// OAuth refresh token for renewing access.
    pub refresh_token: Option<String>,

    /// Timestamp when the access token expires.
    pub token_expires_at: Option<DateTime<Utc>>,

    /// Whether this account is actively syncing.
    pub is_active: bool,

    /// When this account was added to the database.
    pub created_at: DateTime<Utc>,

    /// When this account was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Account {
    /// Creates a new account with the given details.
    ///
    /// The account ID is automatically generated, and timestamps are set to now.
    pub fn new(
        provider: ProviderId,
        email: String,
        access_token: String,
        refresh_token: Option<String>,
        token_expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: AccountId::new(),
            provider,
            email,
            access_token,
            refresh_token,
            token_expires_at,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Maps a database row to an Account.
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        // Parse provider from string
        let provider_str: String = row.get(1)?;
        let provider = match provider_str.as_str() {
            "googledrive" => ProviderId::GoogleDrive,
            "dropbox" => ProviderId::Dropbox,
            "onedrive" => ProviderId::OneDrive,
            _ => return Err(rusqlite::Error::InvalidQuery),
        };

        // Parse timestamps
        let token_expires_at: Option<i64> = row.get(5)?;
        let created_at: i64 = row.get(7)?;
        let updated_at: i64 = row.get(8)?;

        Ok(Self {
            id: AccountId::from_string(row.get::<_, String>(0)?),
            provider,
            email: row.get(2)?,
            access_token: row.get(3)?,
            refresh_token: row.get(4)?,
            token_expires_at: token_expires_at.map(|ts| DateTime::from_timestamp(ts, 0).unwrap()),
            is_active: row.get::<_, i32>(6)? != 0,
            created_at: DateTime::from_timestamp(created_at, 0).unwrap(),
            updated_at: DateTime::from_timestamp(updated_at, 0).unwrap(),
        })
    }
}

/// Creates a new account in the database.
///
/// # Arguments
///
/// * `conn` - Database connection
/// * `account` - The account to create
///
/// # Returns
///
/// * `Ok(AccountId)` - The ID of the created account
/// * `Err(DbError::Conflict)` - If an account with the same provider and email already exists
pub fn create_account(conn: &Connection, account: &Account) -> DbResult<AccountId> {
    conn.execute(
        "INSERT INTO accounts (
            id, provider, email, access_token, refresh_token,
            token_expires_at, is_active, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            account.id.to_string(),
            match account.provider {
                ProviderId::GoogleDrive => "googledrive",
                ProviderId::Dropbox => "dropbox",
                ProviderId::OneDrive => "onedrive",
            },
            account.email,
            account.access_token,
            account.refresh_token,
            account.token_expires_at.map(|dt| dt.timestamp()),
            account.is_active as i32,
            account.created_at.timestamp(),
            account.updated_at.timestamp(),
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            DbError::Conflict("Account with this provider and email already exists".to_string())
        }
        _ => DbError::from(e),
    })?;

    Ok(account.id.clone())
}

/// Retrieves an account by its ID.
///
/// # Returns
///
/// * `Ok(Some(Account))` - The account if found
/// * `Ok(None)` - If no account with this ID exists
pub fn get_account_by_id(conn: &Connection, id: &AccountId) -> DbResult<Option<Account>> {
    conn.query_row(
        "SELECT id, provider, email, access_token, refresh_token,
                token_expires_at, is_active, created_at, updated_at
         FROM accounts WHERE id = ?1",
        params![id.to_string()],
        Account::from_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Retrieves all accounts from the database.
///
/// # Arguments
///
/// * `active_only` - If true, only returns active accounts
pub fn list_accounts(conn: &Connection, active_only: bool) -> DbResult<Vec<Account>> {
    let sql = if active_only {
        "SELECT id, provider, email, access_token, refresh_token,
                token_expires_at, is_active, created_at, updated_at
         FROM accounts WHERE is_active = 1
         ORDER BY created_at DESC"
    } else {
        "SELECT id, provider, email, access_token, refresh_token,
                token_expires_at, is_active, created_at, updated_at
         FROM accounts
         ORDER BY created_at DESC"
    };

    let mut stmt = conn.prepare(sql)?;
    let accounts = stmt
        .query_map([], Account::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(accounts)
}

/// Updates an existing account.
///
/// # Returns
///
/// * `Ok(true)` - If the account was updated
/// * `Ok(false)` - If no account with this ID exists
pub fn update_account(conn: &Connection, account: &Account) -> DbResult<bool> {
    let updated_account = Account {
        updated_at: Utc::now(),
        ..account.clone()
    };

    let rows_affected = conn.execute(
        "UPDATE accounts
         SET provider = ?2, email = ?3, access_token = ?4, refresh_token = ?5,
             token_expires_at = ?6, is_active = ?7, updated_at = ?8
         WHERE id = ?1",
        params![
            updated_account.id.to_string(),
            match updated_account.provider {
                ProviderId::GoogleDrive => "googledrive",
                ProviderId::Dropbox => "dropbox",
                ProviderId::OneDrive => "onedrive",
            },
            updated_account.email,
            updated_account.access_token,
            updated_account.refresh_token,
            updated_account.token_expires_at.map(|dt| dt.timestamp()),
            updated_account.is_active as i32,
            updated_account.updated_at.timestamp(),
        ],
    )?;

    Ok(rows_affected > 0)
}

/// Deletes an account by its ID.
///
/// # Returns
///
/// * `Ok(true)` - If the account was deleted
/// * `Ok(false)` - If no account with this ID exists
pub fn delete_account(conn: &Connection, id: &AccountId) -> DbResult<bool> {
    let rows_affected = conn.execute(
        "DELETE FROM accounts WHERE id = ?1",
        params![id.to_string()],
    )?;
    Ok(rows_affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionManager, Migration, Migrator};

    fn setup_test_db() -> Connection {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        // Apply migration
        let migrator = Migrator::new(vec![Migration {
            version: 1,
            description: "Create accounts table",
            sql: ACCOUNTS_MIGRATION,
        }]);
        migrator.migrate(&conn).unwrap();

        conn
    }

    fn create_test_account() -> Account {
        Account::new(
            ProviderId::GoogleDrive,
            "test@example.com".to_string(),
            "access_token_123".to_string(),
            Some("refresh_token_456".to_string()),
            Some(Utc::now() + chrono::Duration::hours(1)),
        )
    }

    #[test]
    fn test_create_account_success() {
        let conn = setup_test_db();
        let account = create_test_account();

        let result = create_account(&conn, &account);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), account.id);
    }

    #[test]
    fn test_create_account_duplicate_provider_email() {
        let conn = setup_test_db();
        let account = create_test_account();

        // First insert should succeed
        create_account(&conn, &account).unwrap();

        // Second insert with same provider and email should fail
        let duplicate = Account::new(
            ProviderId::GoogleDrive,
            "test@example.com".to_string(),
            "different_token".to_string(),
            None,
            None,
        );

        let result = create_account(&conn, &duplicate);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DbError::Conflict(_)));
    }

    #[test]
    fn test_create_account_different_provider_same_email() {
        let conn = setup_test_db();
        let account1 = create_test_account();

        create_account(&conn, &account1).unwrap();

        // Same email but different provider should succeed
        let account2 = Account::new(
            ProviderId::Dropbox,
            "test@example.com".to_string(),
            "access_token_789".to_string(),
            None,
            None,
        );

        let result = create_account(&conn, &account2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_account_by_id_found() {
        let conn = setup_test_db();
        let account = create_test_account();

        create_account(&conn, &account).unwrap();

        let result = get_account_by_id(&conn, &account.id).unwrap();
        assert!(result.is_some());

        let retrieved = result.unwrap();
        assert_eq!(retrieved.id, account.id);
        assert_eq!(retrieved.provider, account.provider);
        assert_eq!(retrieved.email, account.email);
        assert_eq!(retrieved.access_token, account.access_token);
        assert_eq!(retrieved.refresh_token, account.refresh_token);
        assert!(retrieved.is_active);
    }

    #[test]
    fn test_get_account_by_id_not_found() {
        let conn = setup_test_db();
        let fake_id = AccountId::new();

        let result = get_account_by_id(&conn, &fake_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_accounts_empty() {
        let conn = setup_test_db();

        let accounts = list_accounts(&conn, false).unwrap();
        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn test_list_accounts_multiple() {
        let conn = setup_test_db();

        let account1 = create_test_account();
        let account2 = Account::new(
            ProviderId::Dropbox,
            "user2@example.com".to_string(),
            "token2".to_string(),
            None,
            None,
        );

        create_account(&conn, &account1).unwrap();
        create_account(&conn, &account2).unwrap();

        let accounts = list_accounts(&conn, false).unwrap();
        assert_eq!(accounts.len(), 2);
    }

    #[test]
    fn test_list_accounts_active_only() {
        let conn = setup_test_db();

        let mut account1 = create_test_account();
        let account2 = Account::new(
            ProviderId::Dropbox,
            "user2@example.com".to_string(),
            "token2".to_string(),
            None,
            None,
        );

        create_account(&conn, &account1).unwrap();
        create_account(&conn, &account2).unwrap();

        // Deactivate account1
        account1.is_active = false;
        update_account(&conn, &account1).unwrap();

        let all_accounts = list_accounts(&conn, false).unwrap();
        assert_eq!(all_accounts.len(), 2);

        let active_accounts = list_accounts(&conn, true).unwrap();
        assert_eq!(active_accounts.len(), 1);
        assert_eq!(active_accounts[0].id, account2.id);
    }

    #[test]
    fn test_update_account_success() {
        let conn = setup_test_db();
        let mut account = create_test_account();

        create_account(&conn, &account).unwrap();

        // Update the account
        account.email = "updated@example.com".to_string();
        account.access_token = "new_token".to_string();
        account.is_active = false;

        let updated = update_account(&conn, &account).unwrap();
        assert!(updated);

        // Verify the update
        let retrieved = get_account_by_id(&conn, &account.id).unwrap().unwrap();
        assert_eq!(retrieved.email, "updated@example.com");
        assert_eq!(retrieved.access_token, "new_token");
        assert!(!retrieved.is_active);
    }

    #[test]
    fn test_update_account_not_found() {
        let conn = setup_test_db();
        let account = create_test_account();

        // Don't create the account, just try to update it
        let updated = update_account(&conn, &account).unwrap();
        assert!(!updated);
    }

    #[test]
    fn test_update_account_updates_timestamp() {
        let conn = setup_test_db();
        let mut account = create_test_account();

        create_account(&conn, &account).unwrap();

        let original_updated_at = account.updated_at;

        // Wait to ensure timestamp changes (SQLite timestamps are in seconds)
        std::thread::sleep(std::time::Duration::from_secs(1));

        account.email = "changed@example.com".to_string();
        update_account(&conn, &account).unwrap();

        let retrieved = get_account_by_id(&conn, &account.id).unwrap().unwrap();
        assert!(retrieved.updated_at > original_updated_at);
    }

    #[test]
    fn test_delete_account_success() {
        let conn = setup_test_db();
        let account = create_test_account();

        create_account(&conn, &account).unwrap();

        let deleted = delete_account(&conn, &account.id).unwrap();
        assert!(deleted);

        // Verify deletion
        let result = get_account_by_id(&conn, &account.id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_account_not_found() {
        let conn = setup_test_db();
        let fake_id = AccountId::new();

        let deleted = delete_account(&conn, &fake_id).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_account_new_sets_timestamps() {
        let before = Utc::now();
        let account = create_test_account();
        let after = Utc::now();

        assert!(account.created_at >= before && account.created_at <= after);
        assert!(account.updated_at >= before && account.updated_at <= after);
        assert_eq!(account.created_at, account.updated_at);
    }

    #[test]
    fn test_account_new_generates_unique_ids() {
        let account1 = create_test_account();
        let account2 = create_test_account();

        assert_ne!(account1.id, account2.id);
    }

    #[test]
    fn test_account_new_defaults_to_active() {
        let account = create_test_account();
        assert!(account.is_active);
    }

    #[test]
    fn test_all_provider_types_supported() {
        let conn = setup_test_db();

        let providers = vec![
            ProviderId::GoogleDrive,
            ProviderId::Dropbox,
            ProviderId::OneDrive,
        ];

        for (i, provider) in providers.iter().enumerate() {
            let account = Account::new(
                *provider,
                format!("user{}@example.com", i),
                format!("token{}", i),
                None,
                None,
            );

            create_account(&conn, &account).unwrap();

            let retrieved = get_account_by_id(&conn, &account.id).unwrap().unwrap();
            assert_eq!(retrieved.provider, *provider);
        }
    }

    #[test]
    fn test_token_expiration_optional() {
        let conn = setup_test_db();

        let account_with_expiry = Account::new(
            ProviderId::GoogleDrive,
            "with@example.com".to_string(),
            "token".to_string(),
            None,
            Some(Utc::now() + chrono::Duration::hours(1)),
        );

        let account_without_expiry = Account::new(
            ProviderId::Dropbox,
            "without@example.com".to_string(),
            "token".to_string(),
            None,
            None,
        );

        create_account(&conn, &account_with_expiry).unwrap();
        create_account(&conn, &account_without_expiry).unwrap();

        let retrieved1 = get_account_by_id(&conn, &account_with_expiry.id)
            .unwrap()
            .unwrap();
        assert!(retrieved1.token_expires_at.is_some());

        let retrieved2 = get_account_by_id(&conn, &account_without_expiry.id)
            .unwrap()
            .unwrap();
        assert!(retrieved2.token_expires_at.is_none());
    }

    #[test]
    fn test_refresh_token_optional() {
        let conn = setup_test_db();

        let account = Account::new(
            ProviderId::GoogleDrive,
            "test@example.com".to_string(),
            "access".to_string(),
            None, // No refresh token
            None,
        );

        create_account(&conn, &account).unwrap();

        let retrieved = get_account_by_id(&conn, &account.id).unwrap().unwrap();
        assert!(retrieved.refresh_token.is_none());
    }
}
