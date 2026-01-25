# CloudSync Implementation Plan

This document tracks the phased implementation of CloudSync based on the PRD.

---

## Phase 1: Foundation (PRD Weeks 1-4)

### 1.1 Project Scaffolding ✅ COMPLETE
- [x] Create Cargo workspace structure
- [x] Set up `cloudsync-core` crate with Cargo.toml
- [x] Set up `cloudsync-config` crate
- [x] Set up `cloudsync-db` crate
- [x] Set up `cloudsync-ipc` crate
- [x] Set up `cloudsync-providers` crate
- [x] Set up `cloudsync-daemon` crate
- [x] Verify workspace builds with `cargo build`

### 1.2 Core Types & Traits ✅ COMPLETE
- [x] Define error types (`Error`, `Result`)
- [x] Define `FileState` enum with tests
- [x] Define core types (`CloudPath`, `FileId`, `CloudItem`, etc.)
- [x] Define `CloudProvider` trait
- [x] Verify all tests pass with `cargo test`

### 1.3 Configuration Management ⬅️ NEXT
- [ ] Define config structures (accounts, sync settings, etc.)
- [ ] Implement TOML config file loading/saving
- [ ] Implement XDG directory compliance for Linux
- [ ] Add config validation
- [ ] Write tests for config module

### 1.4 Database Layer
- [ ] Create SQLite schema (accounts, files, sync_queue, etc.)
- [ ] Implement database connection management
- [ ] Implement basic CRUD operations for accounts
- [ ] Implement basic CRUD operations for files
- [ ] Write tests with in-memory SQLite

### 1.5 CI/CD Setup
- [ ] Create GitHub Actions workflow for tests
- [ ] Add clippy linting to CI
- [ ] Add rustfmt check to CI
- [ ] Set up code coverage (optional)

---

## Phase 2: Google Drive Integration (PRD Weeks 2-4)

### 2.1 OAuth Flow
- [ ] Implement OAuth 2.0 authorization URL generation
- [ ] Implement token exchange
- [ ] Implement token refresh
- [ ] Store tokens in system keyring
- [ ] Write tests for OAuth flow

### 2.2 Google Drive API Wrapper
- [ ] Implement `CloudProvider` trait for Google Drive
- [ ] Implement `list_folder`
- [ ] Implement `download`
- [ ] Implement `upload`
- [ ] Implement `delete`
- [ ] Implement `get_changes` (incremental sync)
- [ ] Write integration tests (optional, requires credentials)

### 2.3 File Watcher
- [ ] Set up `notify` crate for filesystem watching
- [ ] Implement event debouncing
- [ ] Map filesystem events to sync operations
- [ ] Write tests for file watcher

### 2.4 Sync Queue
- [ ] Implement sync queue with priority
- [ ] Implement retry logic with exponential backoff
- [ ] Implement conflict detection
- [ ] Write tests for sync queue

---

## Phase 3: IPC & Shell Integration (PRD Weeks 4-7)

### 3.1 IPC Protocol
- [ ] Define IPC message types (JSON)
- [ ] Implement Unix socket server
- [ ] Implement request/response handling
- [ ] Write tests for IPC protocol

### 3.2 Nautilus Extension (Linux)
- [ ] Create Python extension skeleton
- [ ] Implement status emblem provider
- [ ] Implement context menu provider
- [ ] Test with Nautilus file manager

### 3.3 Dolphin Plugin (Linux)
- [ ] Research KDE plugin requirements
- [ ] Implement overlay icon plugin
- [ ] Implement context menu plugin
- [ ] Test with Dolphin file manager

---

## Phase 4: System Tray & Polish (PRD Week 8)

### 4.1 System Tray Application
- [ ] Set up `iced` GUI framework
- [ ] Implement tray icon with status
- [ ] Implement status menu
- [ ] Implement settings window

### 4.2 Bandwidth Throttling
- [ ] Implement upload/download rate limiting
- [ ] Add configuration options
- [ ] Write tests for throttling

---

## Future Phases (Not in Scope for Initial Setup)

- Phase 5: Dropbox Provider
- Phase 6: OneDrive Provider
- Phase 7: Windows Port
- Phase 8: macOS Port

---

## Current Focus

**Next: Phase 1.3 - Configuration Management**

Completed:
- Phase 1.1: Project scaffolding with 6 crates
- Phase 1.2: Core types and CloudProvider trait (56 tests passing)
