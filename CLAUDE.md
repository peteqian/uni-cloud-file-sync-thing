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

## Documentation and Progress Tracking

### Project Workflow

- Do NOT include project progress or session summaries in this CLAUDE.md file.

- You should move the issue to "In Progress" -> "In Review" -> "Done"

Once you are done with implementation, commit the code, attach detailed progress reports as comments to the relevant GitHub issues.

Move the issue to "In Review" and proceed with testing.

Once all tests have passed, move the issue to "Done".

Each issue comment should include:

- Deliverables and implementation details
- Test coverage and quality metrics
- Architecture decisions made
- Key implementation insights
- Code quality results (tests passing, clippy, rustfmt)
- Commit Hash

### Learning Insights While Coding

**Always incorporate educational insights** while implementing features. Use the "★ Insight" format to explain:

- Architecture decisions and trade-offs
- Technology-specific constraints (e.g., SQLite WAL mode limitations)
- Design patterns being applied
- Performance or security considerations
- Why certain approaches were chosen over alternatives

Insert insights DURING implementation, not just at the end. This helps explain the "why" behind technical decisions and documents the reasoning for future reference.
