# CONVENTIONS.md: Engineering & Coding Conventions for ssync

This document establishes the development standards, code style, architecture guidelines, and commit policies for the **ssync** project.

---

## 1. Rust Language & Toolchain Policy

- **Rust Edition**: 2024 (or 2021 where 2024 features are not required).
- **MSRV**: Rust 1.85.0.
- **Compiler Warnings**: Must compile cleanly with `cargo clippy --workspace --all-targets -- -D warnings`.
- **Formatting**: Strictly enforced via `cargo fmt --check` with standard `rustfmt.toml`.

---

## 2. Unsafe Code Policy

- **Principle**: `#![forbid(unsafe_code)]` in all crates by default.
- **Exception**: Minimal, audited `unsafe` blocks are allowed ONLY where strictly required for POSIX systems calls (e.g., `libc` / `nix` bindings for `lutimes`, `fchmod`, `lchown`, or SIMD intrinsics).
- **Requirements for any `unsafe` block**:
  - Must include a `// SAFETY:` comment explaining the preconditions and why the call is memory-safe.
  - Must be encapsulated within safe abstraction functions in `ssync-fs` or `ssync-core`.

---

## 3. Error Handling Architecture

- **Library Crates (`ssync-core`, `ssync-fs`, `ssync-proto`)**:
  - Use `thiserror` to define precise, structured error enums (e.g. `CoreError`, `FsError`, `ProtoError`).
  - Do NOT call `panic!`, `unwrap()`, or `expect()` on user-influenced inputs or runtime I/O.
  - Return `Result<T, Error>` for all fallible operations.
- **Application Crate (`ssync-cli`)**:
  - Use `anyhow` or top-level structured error reporting.
  - Map errors directly to upstream `RERR_*` exit codes (e.g., `1` for syntax error, `11` for file I/O error, `23` for partial transfer, `24` for vanished files).

---

## 4. Module & Crate Layout

- Workspace structure:
  - `crates/ssync-core`: Pure algorithms, checksums, hash tables, and delta matching logic (independent of filesystem).
  - `crates/ssync-fs`: Filesystem interactions, directory traversal, attribute setting, and filter rules.
  - `crates/ssync-proto`: Wire protocol serialization, varints, and multiplexed streams.
  - `crates/ssync-cli`: Binary entry point, argument parsing, CLI presentation, and process orchestration.
- Module naming:
  - Keep modules focused and files reasonably sized (< 600 lines where practical).
  - Use idiomatic Rust naming conventions (snake_case for functions and modules, CamelCase for types).

---

## 5. Testing Strategy

1. **Unit Tests**:
   - Every algorithm (checksum calculations, varint encoding, wildmatching, path sanitization) must have co-located unit tests (`#[test]`).
2. **Property-Based Tests**:
   - Use `proptest` for the rolling checksum (`Checksum1`) and delta reconstructor to verify that arbitrary byte modifications reconstruct bit-identically.
3. **Integration Tests**:
   - Live filesystem sync scenarios located in `tests/` using `tempfile`.
4. **Upstream Testsuite Wrapper**:
   - Execute tests using `scripts/run_upstream_tests.sh <test_name>` to validate behavioral parity with rsync.
5. **Differential Testing**:
   - Validate edge cases against the reference rsync binary (`/f/rsync/old_versions/rsync_3.4.1` or target build) using `testsuite/abdiff.py`.

---

## 6. Commit Message Style

We follow the Conventional Commits specification:
- `<type>(<scope>): <short description in imperative present tense>`
- Examples:
  - `feat(core): implement rolling checksum and block hash table`
  - `fix(fs): preserve nanosecond timestamps on directory mtime`
  - `test(upstream): wire up runner for testsuite hands and dirs`
  - `docs(plan): add detailed phase 2 acceptance criteria`
- Keep commit history atomic, cohesive, and bisectable.
