# Rust Development Guidelines for Claude

This guide helps Claude AI efficiently develop Rust projects using test-driven development (TDD) and the 80/20 principle. Focus on writing tests before code to ensure quality and maintainability while moving quickly on the most important aspects.

## Core Development Rules

### 1. **Test-Driven Development: Tests Before Code**
**Always** write unit tests first, then implement the code to pass those tests. Create test modules with `#[cfg(test)]` and individual tests with `#[test]`. Run `cargo test` to verify the test fails initially, implement the feature, then confirm the test passes. This is non-negotiable for quality code.

### 2. **Leverage Cargo's Built-in Tools**
Use the Rust toolchain effectively: `cargo build` to compile, `cargo test` to run tests, `cargo clippy` for lint checks, and `cargo fmt` to auto-format code. Run these commands frequently to catch issues early and maintain code quality with minimal effort.

### 3. **Embrace Rust's Type System and Error Handling**
Use `Result<T, E>` for fallible operations and `Option<T>` for nullable values. Never use `unwrap()` or `expect()` in production code paths—always handle errors explicitly. Use the `?` operator to propagate errors cleanly up the call stack.

### 4. **Write Modular, Testable Functions**
Keep functions focused on a single responsibility. Prefer pure functions that don't mutate state. Structure code in small modules with clear boundaries. This makes unit testing straightforward and code easier to reason about.

### 5. **Be Selective with Dependencies**
Only add external crates when they provide significant value. Check the crate's maintenance status, documentation quality, and community adoption on crates.io. Fewer dependencies mean faster builds, smaller binaries, and fewer security concerns.

## Code Quality Guidelines

### Readable Variable Names
Use descriptive, self-documenting variable names to avoid the need for inline comments. Names should clearly express intent and purpose. Prefer `user_authentication_token` over `uat` or `token`.

### Prefer Early Returns
Use early returns to reduce nesting and improve readability. Exit functions as soon as error conditions or edge cases are detected instead of wrapping the entire function in conditional logic.

### Balance Abstraction and File Size
Avoid over-abstraction that makes code harder to follow, but also don't create monolithic files. Keep files under 1000 lines of code by extracting logical modules. If logic cannot be reasonably extracted, exceeding this limit is acceptable.

## Test-Driven Development Cycle

**Red → Green → Refactor**: Write a failing test (Red), write minimal code to pass it (Green), improve the code quality while keeping tests passing (Refactor). Repeat this cycle for each feature or bug fix.

---

## Project Progress

### Session 1: Foundation Setup (Jan 2026)

**Completed Phases:**
- **Phase 1.1: Project Scaffolding** - Created Cargo workspace with 6 modular crates
- **Phase 1.2: Core Types & Traits** - Defined CloudProvider trait, FileState enum, core types
- **Phase 1.3: Configuration Management** - XDG-compliant paths, TOML config system

**Deliverables:**
- Cargo workspace with `cloudsync-core`, `cloudsync-config`, `cloudsync-db`, `cloudsync-ipc`, `cloudsync-providers`, `cloudsync-daemon`
- CloudProvider trait for unified cloud storage abstraction
- FileState enum with 8 sync states (Synced, CloudOnly, Syncing, Pending, Error, Excluded, OfflineModified, Conflict)
- Core types: CloudPath, FileId, CloudItem, TransferProgress, ChangeList
- IPC message protocol (Request/Response types for shell extensions)
- Config system with GeneralConfig, SyncConfig, ConflictsConfig
- CloudSyncPaths helper for XDG-compliant directories (config, data, cache, runtime)
- 75 passing unit tests across workspace
- TODO.md with phased implementation plan
- GitHub Project setup with 4 milestones and 42 issues using Angular commit format

**Test Coverage:**
- 19 tests in `cloudsync-config`
- 44 tests in `cloudsync-core`
- 4 tests in `cloudsync-ipc`
- 3 tests in `cloudsync-daemon`
- 2 tests in `cloudsync-db` (placeholder)

**Next Steps:**
- Phase 1.4: Database Layer (SQLite schema, connection management, CRUD operations)
- Phase 1.5: CI/CD Setup (GitHub Actions, clippy, rustfmt)

**Architecture Decisions:**
- Modular crate structure for separation of concerns
- Provider trait abstraction for multi-cloud support
- XDG Base Directory compliance for Linux
- JSON over Unix sockets for IPC (shell extensions ↔ daemon)
- Test-first development with comprehensive coverage
