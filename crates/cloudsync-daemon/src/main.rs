//! CloudSync Daemon
//!
//! This is the main entry point for the CloudSync background service.
//! The daemon manages file synchronization with cloud providers and
//! responds to IPC requests from shell extensions.

use clap::Parser;
use cloudsync_config::{CloudSyncPaths, Config};
use cloudsync_core::types::AccountId;
use cloudsync_db::{Database, Migration};
use cloudsync_providers::gdrive::{GoogleDriveClient, GoogleDriveProvider, OAuthConfig};
use cloudsync_sync::{SyncEngine, SyncQueue};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;
use yup_oauth2::InstalledFlowReturnMethod;

/// CloudSync - Unified cloud storage synchronization daemon
#[derive(Parser, Debug)]
#[command(name = "cloudsync")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,

    /// Path to configuration file
    #[arg(short, long)]
    config: Option<String>,

    /// Skip initial sync (only run periodic refresh)
    #[arg(long)]
    skip_initial_sync: bool,

    /// Refresh interval in seconds (default: 60)
    #[arg(long, default_value = "60")]
    refresh_interval: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("CloudSync daemon starting...");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // Load .env file if present (for OAuth credentials)
    dotenvy::dotenv().ok();

    // Initialize paths
    let paths = CloudSyncPaths::new()
        .ok_or_else(|| anyhow::anyhow!("Failed to determine CloudSync directory paths"))?;
    info!("Config directory: {}", paths.config_dir().display());
    info!("Database path: {}", paths.database_path().display());

    // Load configuration
    let config = Config::load_from_file(paths.config_file())
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    info!("Loaded configuration");
    info!("Sync root: {}", config.sync.root_folder.display());

    // Create provider root directory
    let provider_root = config
        .sync
        .ensure_provider_root("gdrive")
        .map_err(|e| anyhow::anyhow!("Failed to create provider root: {}", e))?;
    info!("Provider root: {}", provider_root.display());

    // Initialize database
    let migrations = vec![
        Migration {
            version: 1,
            description: "Create accounts table",
            sql: cloudsync_db::ACCOUNTS_MIGRATION,
        },
        Migration {
            version: 2,
            description: "Create files table",
            sql: cloudsync_db::FILES_MIGRATION,
        },
    ];
    let db = Database::open(paths.database_path(), migrations)
        .map_err(|e| anyhow::anyhow!("Failed to open database: {}", e))?;
    info!("Database initialized");

    // Initialize Google Drive provider
    info!("Initializing Google Drive provider...");
    let gdrive_client = create_gdrive_client(paths.token_cache_path()).await?;
    let provider = GoogleDriveProvider::new(gdrive_client)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create Google Drive provider: {}", e))?;
    info!("Google Drive provider initialized");

    // Initialize sync engine
    let queue = Arc::new(SyncQueue::new());
    let engine = Arc::new(SyncEngine::new(
        Arc::new(provider),
        Arc::new(db),
        queue.clone(),
        AccountId::new(), // TODO: Load account ID from database
    ));

    // Perform initial sync if not skipped
    if !args.skip_initial_sync {
        info!("Starting initial sync...");
        match engine
            .initial_sync(&cloudsync_core::types::CloudPath::root())
            .await
        {
            Ok(count) => info!("Initial sync queued {} files", count),
            Err(e) => error!("Initial sync failed: {}", e),
        }

        // Execute queued operations
        info!("Executing queued operations...");
        match engine.execute_operations(&provider_root, None).await {
            Ok(count) => info!("Executed {} operations", count),
            Err(e) => error!("Operation execution failed: {}", e),
        }
    }

    // Start periodic refresh loop
    info!(
        "Starting periodic refresh loop (interval: {} seconds)",
        args.refresh_interval
    );
    let mut refresh_timer = interval(Duration::from_secs(args.refresh_interval));

    loop {
        refresh_timer.tick().await;

        info!("Running periodic refresh...");

        // TODO: Load the last sync cursor from database
        let cursor = None; // For MVP, always start fresh

        // Perform refresh sync
        match engine.refresh_sync(cursor).await {
            Ok((queued, new_cursor)) => {
                info!("Refresh sync queued {} files", queued);
                info!("New cursor: {}", new_cursor);
                // TODO: Store new_cursor in database for next sync
            }
            Err(e) => error!("Refresh sync failed: {}", e),
        }

        // Execute queued operations
        match engine.execute_operations(&provider_root, None).await {
            Ok(count) => info!("Executed {} operations", count),
            Err(e) => error!("Operation execution failed: {}", e),
        }
    }
}

/// Creates a Google Drive client with OAuth authentication.
///
/// This function requires GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET environment
/// variables to be set. On first run, it will open a browser for OAuth authentication.
async fn create_gdrive_client(token_cache_path: PathBuf) -> anyhow::Result<GoogleDriveClient> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_ID environment variable not set"))?;

    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_SECRET environment variable not set"))?;

    let oauth_config = OAuthConfig::default_gdrive(client_id, client_secret);

    let client = GoogleDriveClient::new(
        oauth_config,
        Some(token_cache_path),
        InstalledFlowReturnMethod::HTTPRedirect,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create Google Drive client: {}", e))?;

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parse_defaults() {
        let args = Args::parse_from(["cloudsync"]);
        assert!(!args.verbose);
        assert!(!args.foreground);
        assert!(args.config.is_none());
        assert!(!args.skip_initial_sync);
        assert_eq!(args.refresh_interval, 60);
    }

    #[test]
    fn args_parse_verbose() {
        let args = Args::parse_from(["cloudsync", "--verbose"]);
        assert!(args.verbose);
    }

    #[test]
    fn args_parse_config() {
        let args = Args::parse_from(["cloudsync", "--config", "/path/to/config.toml"]);
        assert_eq!(args.config, Some("/path/to/config.toml".to_string()));
    }

    #[test]
    fn args_parse_skip_initial_sync() {
        let args = Args::parse_from(["cloudsync", "--skip-initial-sync"]);
        assert!(args.skip_initial_sync);
    }

    #[test]
    fn args_parse_refresh_interval() {
        let args = Args::parse_from(["cloudsync", "--refresh-interval", "120"]);
        assert_eq!(args.refresh_interval, 120);
    }
}
