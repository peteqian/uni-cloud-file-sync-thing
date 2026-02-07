//! Sync cursor persistence for incremental change detection.
//!
//! Stores the change cursor per account so incremental sync
//! survives daemon restarts.

use crate::{DbError, DbResult};
use rusqlite::{params, Connection};

/// Database migration for the sync_cursors table.
pub const SYNC_CURSORS_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS sync_cursors (
    account_id TEXT PRIMARY KEY NOT NULL,
    cursor TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);
"#;

/// Inserts or updates a sync cursor for the given account.
pub fn upsert_cursor(conn: &Connection, account_id: &str, cursor: &str) -> DbResult<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO sync_cursors (account_id, cursor, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(account_id) DO UPDATE SET cursor = ?2, updated_at = ?3",
        params![account_id, cursor, now],
    )?;
    Ok(())
}

/// Retrieves the sync cursor for the given account.
pub fn get_cursor(conn: &Connection, account_id: &str) -> DbResult<Option<String>> {
    conn.query_row(
        "SELECT cursor FROM sync_cursors WHERE account_id = ?1",
        params![account_id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Deletes the sync cursor for the given account.
///
/// Returns true if a row was deleted, false if no cursor existed.
pub fn delete_cursor(conn: &Connection, account_id: &str) -> DbResult<bool> {
    let rows = conn.execute(
        "DELETE FROM sync_cursors WHERE account_id = ?1",
        params![account_id],
    )?;
    Ok(rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{accounts, ConnectionManager, Migration, Migrator, ACCOUNTS_MIGRATION};
    use cloudsync_core::types::ProviderId;

    fn setup_test_db() -> Connection {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        let migrator = Migrator::new(vec![
            Migration {
                version: 1,
                description: "Create accounts table",
                sql: ACCOUNTS_MIGRATION,
            },
            Migration {
                version: 2,
                description: "Create sync_cursors table",
                sql: SYNC_CURSORS_MIGRATION,
            },
        ]);
        migrator.migrate(&conn).unwrap();

        conn
    }

    fn create_test_account(conn: &Connection) -> String {
        let account = accounts::Account::new(
            ProviderId::GoogleDrive,
            "test@example.com".to_string(),
            "access_token".to_string(),
            None,
            None,
        );
        accounts::create_account(conn, &account).unwrap();
        account.id.to_string()
    }

    #[test]
    fn test_upsert_cursor_insert() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let result = upsert_cursor(&conn, &account_id, "cursor_abc");
        assert!(result.is_ok());

        let cursor = get_cursor(&conn, &account_id).unwrap();
        assert_eq!(cursor, Some("cursor_abc".to_string()));
    }

    #[test]
    fn test_upsert_cursor_update() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        upsert_cursor(&conn, &account_id, "cursor_v1").unwrap();
        upsert_cursor(&conn, &account_id, "cursor_v2").unwrap();

        let cursor = get_cursor(&conn, &account_id).unwrap();
        assert_eq!(cursor, Some("cursor_v2".to_string()));
    }

    #[test]
    fn test_get_cursor_not_found() {
        let conn = setup_test_db();

        let cursor = get_cursor(&conn, "nonexistent_account").unwrap();
        assert_eq!(cursor, None);
    }

    #[test]
    fn test_delete_cursor_existing() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        upsert_cursor(&conn, &account_id, "cursor_to_delete").unwrap();

        let deleted = delete_cursor(&conn, &account_id).unwrap();
        assert!(deleted);

        let cursor = get_cursor(&conn, &account_id).unwrap();
        assert_eq!(cursor, None);
    }

    #[test]
    fn test_delete_cursor_nonexistent() {
        let conn = setup_test_db();

        let deleted = delete_cursor(&conn, "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_cascade_delete_on_account_removal() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        upsert_cursor(&conn, &account_id, "cursor_abc").unwrap();

        // Delete the account — cursor should cascade-delete
        let account_id_typed = cloudsync_core::types::AccountId::from_string(account_id.clone());
        accounts::delete_account(&conn, &account_id_typed).unwrap();

        let cursor = get_cursor(&conn, &account_id).unwrap();
        assert_eq!(cursor, None);
    }
}
