//! CloudSync Daemon
//!
//! This is the main entry point for the CloudSync background service.
//! The daemon manages file synchronization with cloud providers and
//! responds to IPC requests from shell extensions.

use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

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

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("CloudSync daemon starting...");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // TODO: Initialize components (Phase 1.3+)
    // - Load configuration
    // - Open database
    // - Start IPC server
    // - Start file watcher
    // - Start sync engine

    info!("CloudSync daemon initialized (scaffolding only)");

    // For now, just exit successfully
    // In future phases, this will run the main event loop
    Ok(())
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
}
