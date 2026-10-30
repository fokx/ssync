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

## Supported Flags (Phase 1 & Phase 2)

`ssync` currently supports local recursive file transfer, metadata synchronization, and delta transfers:
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
- `-c, --checksum`: Skip-check based on whole-file strong checksum rather than mtime + size.
- `-u, --update`: Skip files that are newer on the destination.
- `-I, --ignore-times`: Disable quick check; transfer all files.
- `--size-only`: Skip files that match in size, ignoring mtime.
- `--modify-window=N`: Timestamp equivalence window in seconds (default: 0).
- `--inplace`: Update destination files in-place without staging temp file.
- `--append`: Append data onto shorter files without rewriting prefix.
- `--append-verify`: Append data with whole-file checksum verification, re-sending on mismatch.
- `--force`: Force removal of non-empty destination directories when replaced by regular files.
- `--max-size=SIZE`, `--min-size=SIZE`: Transfer only files within byte size thresholds.
- `--block-size=N`: Set fixed checksum block size for delta matching.
- `--stats`: Output transfer statistics (literal data, matched data).
- `-H, --hard-links`: Preserve hard link relationships.
- `--delete, --del`: Delete extraneous destination files.
- `--safe-links`, `--copy-unsafe-links`: Path-escape link handling.
- `-i, --itemize-changes`: 11-character itemized change summary.
- `-v, --verbose`, `-q, --quiet`, `-n, --dry-run`.
