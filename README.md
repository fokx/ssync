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

# Run all workspace unit and integration tests
cargo test --workspace

# Check formatting and clippy
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# Run upstream rsync tests against ssync binary
./scripts/run_upstream_tests.sh hands dirs links relative
```

## Supported Flags (Phase 1)

`ssync` currently supports local recursive file transfer and metadata synchronization:
- `-a, --archive`: Archive mode (equivalent to `-rlptgoD`).
- `-r, --recursive`: Recurse into directories.
- `-l, --links`: Copy symlinks as symlinks.
- `-L, --copy-links`: Dereference symlinks into their referents.
- `-k, --copy-dirlinks`: Treat symlinked directories as directories.
- `-p, --perms`: Preserve permissions / mode.
- `-t, --times`: Preserve modification times (nanosecond precision).
- `-o, --owner`, `-g, --group`: Preserve owner and group (best effort).
- `-d, --dirs`: Transfer directories without recursing into them.
- `-R, --relative`: Relative path replication with `./` cut point support.
- `--no-implied-dirs`: Implied directories created with default permissions.
- `-H, --hard-links`: Preserve hard link relationships.
- `--delete, --del`: Delete extraneous destination files.
- `--safe-links`, `--copy-unsafe-links`: Path-escape link handling.
- `-v, --verbose`, `-q, --quiet`, `-n, --dry-run`.
