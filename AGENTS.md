# Rust Development Guidelines for AI Agents

This document provides essential guidelines for AI agents working on Rust projects. Following the 80/20 principle, these rules capture the most impactful practices that will make your development efficient and maintainable. The primary focus is test-driven development (TDD) to ensure code quality from the start.

## Core Rules

### 1. **Write Unit Tests First, Then Implementation**
Always follow TDD: write failing unit tests before writing any implementation code. Use `#[cfg(test)]` modules and `#[test]` attributes. Run `cargo test` to verify tests fail, then implement code to make them pass. This ensures your code is testable and meets requirements from the start.

### 2. **Use Cargo Ecosystem Tools**
Leverage `cargo` for all development tasks: `cargo build` for compilation, `cargo test` for testing, `cargo clippy` for linting, and `cargo fmt` for formatting. These tools catch 80% of common issues automatically.

### 3. **Handle Errors with Result<T, E>**
Use Rust's `Result` type for error handling instead of panicking. Propagate errors with `?` operator and use `thiserror` or `anyhow` crates for better error messages. This makes code robust and easier to debug.

### 4. **Keep Functions Small and Pure**
Write small, focused functions that do one thing well. Prefer pure functions without side effects when possible. This makes testing easier and code more maintainable.

### 5. **Minimize Dependencies**
Only add dependencies when necessary. Each dependency adds complexity and potential security issues. Check crates.io ratings and recent maintenance before adding new dependencies.

## TDD Workflow

1. **Write test** → 2. **Run test (should fail)** → 3. **Implement code** → 4. **Run test (should pass)** → 5. **Refactor** → 6. **Repeat**
