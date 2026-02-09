# UniCloudST (Unified Cloud Storage Sync Tool)

A fully vibe-coded desktop client for syncing files from cloud storage providers (Google Drive, OneDrive).

## Installation

```bash
git clone https://github.com/peteqian/uni-cloud-file-sync-thing.git
cd uni-cloud-file-sync-thing
cargo build --release
```

## Setup

1. Get Google OAuth credentials from https://console.cloud.google.com/apis/credentials
2. Create Desktop app OAuth client
3. Create `.env` file:
```bash
cp .env.example .env
# Edit .env with your GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET
```

## Running

```bash
# First run (opens browser for auth, syncs all files)
cargo run --release --bin cloudsync -- --verbose --foreground

# Your files will appear in ~/UniCloudST/gdrive/
```

## Options

```bash
cloudsync --help                    # Show all options
cloudsync --skip-initial-sync       # Skip first full sync
cloudsync --refresh-interval 120    # Change refresh rate (seconds)
```

## Current Limitations

- Read-only (no uploads)
- Single account only
- No GUI

## License

MIT
