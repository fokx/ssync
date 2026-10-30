# ssync

A fast, memory-safe, clean-room reimplementation of `rsync` in Rust.

## Goals

- **Algorithmically faithful**: Faithful implementation of the rsync delta-transfer algorithm (rolling checksum + strong hash block matching).
- **Flag compatible**: Drop-in compatible CLI flag subset matching standard `rsync`, verified against upstream rsync regression test suites.
- **Modern & Safe**: Built with modern Rust (Edition 2024, MSRV 1.85.0), strict safety guarantees (`#![forbid(unsafe_code)]` in safe crates), and zero copyleft licensing obligations.

## License

`ssync` is licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or [LICENSE-APACHE](LICENSE-APACHE)).

Upstream reference code (`/f/rsync`, GPL-3.0-or-later) is strictly consulted for behavioral specification and protocol verification; no upstream code or test scripts are copied into this repository. See [docs/DECISIONS.md](docs/DECISIONS.md) for licensing and architectural rationale.

## Documentation

- [RSYNC_NOTES.md](docs/RSYNC_NOTES.md): Comprehensive analysis of rsync protocol versions, file-list wire encoding, delta algorithm, and option interactions.
- [TESTSUITE_MAP.md](docs/TESTSUITE_MAP.md): Complete map and classification of all 374 upstream rsync automated tests.
- [DECISIONS.md](docs/DECISIONS.md): Architectural decisions (licensing, crate layout, dependencies, wire compatibility).
- [PLAN.md](docs/PLAN.md): Phased roadmap with deliverables and acceptance criteria.
- [PROGRESS.md](docs/PROGRESS.md): Persistent progress tracker and backlog.
- [CONVENTIONS.md](docs/CONVENTIONS.md): Coding guidelines, error handling, unsafe policy, and commit style.

## Building & Testing

```bash
# Build all crates in the workspace
cargo build --workspace

# Run all workspace unit tests
cargo test --workspace

# Check formatting and clippy
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# Run upstream rsync tests against ssync binary
./scripts/run_upstream_tests.sh hands
```
