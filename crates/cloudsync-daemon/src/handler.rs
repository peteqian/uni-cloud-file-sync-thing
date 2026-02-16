//! IPC request handler for the CloudSync daemon.
//!
//! Maps incoming IPC requests from shell extensions to database queries
//! and daemon actions (sync, pause, resume).

use async_trait::async_trait;
use cloudsync_core::file_state::FileState;
use cloudsync_core::types::{AccountId, CloudPath, ProviderId};
use cloudsync_db::Database;
use cloudsync_ipc::messages::{FileStatus, Response};
use cloudsync_ipc::path_resolver::PathResolver;
use cloudsync_ipc::{self, Request, RequestHandler};
use tokio::sync::mpsc;
use tracing::debug;

/// Actions the handler sends to the daemon's sync loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonAction {
    Force {
        account_id: AccountId,
        cloud_path: CloudPath,
    },
    Pause {
        provider: Option<ProviderId>,
    },
    Resume {
        provider: Option<ProviderId>,
    },
}

/// Handles IPC requests by querying the database and dispatching actions.
pub struct DaemonHandler {
    db: Database,
    path_resolver: PathResolver,
    /// Mapping of (provider_slug, account_id) for resolving paths to accounts.
    provider_accounts: Vec<(String, AccountId)>,
    action_tx: Option<mpsc::Sender<DaemonAction>>,
}

impl DaemonHandler {
    pub fn new(
        db: Database,
        path_resolver: PathResolver,
        provider_accounts: Vec<(String, AccountId)>,
        action_tx: Option<mpsc::Sender<DaemonAction>>,
    ) -> Self {
        Self {
            db,
            path_resolver,
            provider_accounts,
            action_tx,
        }
    }

    fn lookup_account(&self, provider_slug: &str) -> Option<&AccountId> {
        self.provider_accounts
            .iter()
            .find(|(slug, _)| slug == provider_slug)
            .map(|(_, id)| id)
    }

    fn resolve_and_lookup(&self, path: &str) -> Result<(AccountId, CloudPath, String), Response> {
        let resolved = self
            .path_resolver
            .resolve_local(std::path::Path::new(path))
            .ok_or_else(|| Response::Error {
                message: format!("path outside sync root: {path}"),
            })?;

        let account_id = self
            .lookup_account(&resolved.provider_slug)
            .cloned()
            .ok_or_else(|| Response::Error {
                message: format!("unknown provider: {}", resolved.provider_slug),
            })?;

        Ok((account_id, resolved.cloud_path, resolved.provider_slug))
    }

    fn get_status_for_path(&self, path: &str) -> Response {
        let (account_id, cloud_path, provider_slug) = match self.resolve_and_lookup(path) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let file_result = self
            .db
            .with_conn(|conn| cloudsync_db::get_file_by_cloud_path(conn, &account_id, &cloud_path));

        match file_result {
            Ok(Some(file)) => Response::Status {
                path: path.to_string(),
                state: file.state,
                provider: Some(provider_slug),
                error: None,
            },
            Ok(None) => Response::Error {
                message: format!("file not found: {path}"),
            },
            Err(e) => Response::Error {
                message: format!("database error: {e}"),
            },
        }
    }

    async fn send_action(&self, action: DaemonAction) {
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(action).await;
        }
    }
}

#[async_trait]
impl RequestHandler for DaemonHandler {
    async fn handle(&self, request: Request) -> cloudsync_ipc::Result<Response> {
        debug!(?request, "handling IPC request");

        let response = match request {
            Request::GetStatus { path } => self.get_status_for_path(&path),

            Request::GetStatusBatch { paths } => {
                let statuses = paths
                    .iter()
                    .map(|p| match self.get_status_for_path(p) {
                        Response::Status {
                            path,
                            state,
                            provider,
                            ..
                        } => FileStatus {
                            path,
                            state,
                            provider,
                        },
                        _ => FileStatus {
                            path: p.clone(),
                            state: FileState::Error,
                            provider: None,
                        },
                    })
                    .collect();
                Response::StatusBatch { statuses }
            }

            Request::SetPinned { path, pinned } => {
                let (account_id, cloud_path, _slug) = match self.resolve_and_lookup(&path) {
                    Ok(v) => v,
                    Err(resp) => return Ok(resp),
                };

                let update_result = self.db.with_conn(|conn| {
                    let file =
                        cloudsync_db::get_file_by_cloud_path(conn, &account_id, &cloud_path)?;
                    let Some(mut file) = file else {
                        return Ok(false);
                    };

                    if pinned {
                        if file.state == FileState::CloudOnly {
                            file.state = FileState::Pending;
                            cloudsync_db::update_file(conn, &file)?;
                        }
                    } else {
                        file.state = FileState::CloudOnly;
                        cloudsync_db::update_file(conn, &file)?;
                    }
                    Ok(true)
                });

                match update_result {
                    Ok(true) => {
                        if pinned {
                            self.send_action(DaemonAction::Force {
                                account_id,
                                cloud_path,
                            })
                            .await;
                        }
                        Response::Ok
                    }
                    Ok(false) => Response::Error {
                        message: format!("file not found: {path}"),
                    },
                    Err(e) => Response::Error {
                        message: format!("database error: {e}"),
                    },
                }
            }

            Request::SetCloudOnly { path } => {
                let (account_id, cloud_path, _slug) = match self.resolve_and_lookup(&path) {
                    Ok(v) => v,
                    Err(resp) => return Ok(resp),
                };

                let update_result = self.db.with_conn(|conn| {
                    let file =
                        cloudsync_db::get_file_by_cloud_path(conn, &account_id, &cloud_path)?;
                    let Some(mut file) = file else {
                        return Ok(false);
                    };
                    file.state = FileState::CloudOnly;
                    cloudsync_db::update_file(conn, &file)?;
                    Ok(true)
                });

                match update_result {
                    Ok(true) => Response::Ok,
                    Ok(false) => Response::Error {
                        message: format!("file not found: {path}"),
                    },
                    Err(e) => Response::Error {
                        message: format!("database error: {e}"),
                    },
                }
            }

            Request::ForceSync { path } => {
                let (account_id, cloud_path, _slug) = match self.resolve_and_lookup(&path) {
                    Ok(v) => v,
                    Err(resp) => return Ok(resp),
                };

                self.send_action(DaemonAction::Force {
                    account_id,
                    cloud_path,
                })
                .await;
                Response::Ok
            }

            Request::PauseSync { provider } => {
                let provider_id = provider.as_deref().and_then(ProviderId::from_slug);
                self.send_action(DaemonAction::Pause {
                    provider: provider_id,
                })
                .await;
                Response::Ok
            }

            Request::ResumeSync { provider } => {
                let provider_id = provider.as_deref().and_then(ProviderId::from_slug);
                self.send_action(DaemonAction::Resume {
                    provider: provider_id,
                })
                .await;
                Response::Ok
            }
        };

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cloudsync_core::types::FileId;
    use cloudsync_db::{files::File, Migration, ACCOUNTS_MIGRATION, FILES_MIGRATION};

    fn test_db() -> Database {
        Database::in_memory_with_migrations(vec![
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
        ])
        .unwrap()
    }

    fn test_handler(db: Database) -> DaemonHandler {
        DaemonHandler::new(
            db,
            PathResolver::new("/home/user/UniCloudST"),
            vec![("gdrive".to_string(), AccountId::from_string("acc-1"))],
            None,
        )
    }

    fn test_handler_with_channel(db: Database) -> (DaemonHandler, mpsc::Receiver<DaemonAction>) {
        let (tx, rx) = mpsc::channel(16);
        let handler = DaemonHandler::new(
            db,
            PathResolver::new("/home/user/UniCloudST"),
            vec![("gdrive".to_string(), AccountId::from_string("acc-1"))],
            Some(tx),
        );
        (handler, rx)
    }

    fn seed_account_and_file(db: &Database, state: FileState) {
        db.with_conn(|conn| {
            // Insert account with a known ID using raw SQL so we can control the ID
            let now = Utc::now().timestamp();
            conn.execute(
                "INSERT INTO accounts (id, provider, email, access_token, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
                rusqlite::params!["acc-1", "googledrive", "test@example.com", "token", now, now],
            )?;

            let mut file = File::new(
                AccountId::from_string("acc-1"),
                FileId::new("pf-1"),
                CloudPath::new("/docs/report.pdf"),
                "report.pdf".to_string(),
                Some(1024),
                None,
                false,
                Utc::now(),
            );
            file.state = state;
            cloudsync_db::create_file(conn, &file)?;
            Ok(())
        })
        .unwrap();
    }

    #[tokio::test]
    async fn get_status_existing_file() {
        let db = test_db();
        seed_account_and_file(&db, FileState::Synced);
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::GetStatus {
                path: "/home/user/UniCloudST/gdrive/docs/report.pdf".to_string(),
            })
            .await
            .unwrap();

        match resp {
            Response::Status {
                state, provider, ..
            } => {
                assert_eq!(state, FileState::Synced);
                assert_eq!(provider.as_deref(), Some("gdrive"));
            }
            other => panic!("expected Status, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_status_unknown_path_returns_error() {
        let db = test_db();
        seed_account_and_file(&db, FileState::Synced);
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::GetStatus {
                path: "/home/user/UniCloudST/gdrive/nonexistent.txt".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Error { .. }));
    }

    #[tokio::test]
    async fn get_status_outside_sync_root_returns_error() {
        let db = test_db();
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::GetStatus {
                path: "/tmp/file.txt".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Error { .. }));
    }

    #[tokio::test]
    async fn get_status_batch_mixed_results() {
        let db = test_db();
        seed_account_and_file(&db, FileState::Synced);
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::GetStatusBatch {
                paths: vec![
                    "/home/user/UniCloudST/gdrive/docs/report.pdf".to_string(),
                    "/home/user/UniCloudST/gdrive/missing.txt".to_string(),
                ],
            })
            .await
            .unwrap();

        match resp {
            Response::StatusBatch { statuses } => {
                assert_eq!(statuses.len(), 2);
                assert_eq!(statuses[0].state, FileState::Synced);
                assert_eq!(statuses[1].state, FileState::Error);
            }
            other => panic!("expected StatusBatch, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_pinned_true_on_cloud_only_updates_state() {
        let db = test_db();
        seed_account_and_file(&db, FileState::CloudOnly);
        let handler = test_handler(db.clone());

        let resp = handler
            .handle(Request::SetPinned {
                path: "/home/user/UniCloudST/gdrive/docs/report.pdf".to_string(),
                pinned: true,
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Ok));

        // Verify state was updated
        let file = db
            .with_conn(|conn| {
                cloudsync_db::get_file_by_cloud_path(
                    conn,
                    &AccountId::from_string("acc-1"),
                    &CloudPath::new("/docs/report.pdf"),
                )
            })
            .unwrap()
            .unwrap();
        assert_eq!(file.state, FileState::Pending);
    }

    #[tokio::test]
    async fn set_cloud_only_updates_state() {
        let db = test_db();
        seed_account_and_file(&db, FileState::Synced);
        let handler = test_handler(db.clone());

        let resp = handler
            .handle(Request::SetCloudOnly {
                path: "/home/user/UniCloudST/gdrive/docs/report.pdf".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Ok));

        let file = db
            .with_conn(|conn| {
                cloudsync_db::get_file_by_cloud_path(
                    conn,
                    &AccountId::from_string("acc-1"),
                    &CloudPath::new("/docs/report.pdf"),
                )
            })
            .unwrap()
            .unwrap();
        assert_eq!(file.state, FileState::CloudOnly);
    }

    #[tokio::test]
    async fn force_sync_sends_action() {
        let db = test_db();
        seed_account_and_file(&db, FileState::CloudOnly);
        let (handler, mut rx) = test_handler_with_channel(db);

        let resp = handler
            .handle(Request::ForceSync {
                path: "/home/user/UniCloudST/gdrive/docs/report.pdf".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Ok));

        let action = rx.try_recv().unwrap();
        assert_eq!(
            action,
            DaemonAction::Force {
                account_id: AccountId::from_string("acc-1"),
                cloud_path: CloudPath::new("/docs/report.pdf"),
            }
        );
    }

    #[tokio::test]
    async fn force_sync_without_channel_returns_ok() {
        let db = test_db();
        seed_account_and_file(&db, FileState::CloudOnly);
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::ForceSync {
                path: "/home/user/UniCloudST/gdrive/docs/report.pdf".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Ok));
    }

    #[tokio::test]
    async fn pause_sync_returns_ok() {
        let db = test_db();
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::PauseSync { provider: None })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Ok));
    }

    #[tokio::test]
    async fn resume_sync_returns_ok() {
        let db = test_db();
        let handler = test_handler(db);

        let resp = handler
            .handle(Request::ResumeSync {
                provider: Some("gdrive".to_string()),
            })
            .await
            .unwrap();

        assert!(matches!(resp, Response::Ok));
    }
}
