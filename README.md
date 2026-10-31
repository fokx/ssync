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

## Supported Flags (Phase 1, Phase 2, Phase 3 & Phase 4)

`ssync` currently supports local and remote recursive file transfer, metadata synchronization, delta transfers, advanced filtering/staging, and wire protocol 31:
- Remote synchronization: `[user@]host:path` source and destination syntax over SSH or custom remote shells (`-e, --rsh`, `$RSYNC_RSH`).
- `--server`, `--sender`: Protocol 31 server execution over standard I/O streams with multiplexed framing.
- `-s, --protect-args, --secluded-args`: Protect remote arguments from shell expansion via null-delimited stream.
- `--old-args`: Pass remote arguments unquoted.
- `--rsync-path=PROGRAM`: Specify the remote rsync executable.
- `--max-alloc=SIZE`: Set maximum memory allocation limit (`0` for unlimited).
- `-z, --compress`: Compressed transfer flag parsing.
- `--info=help`, `--debug=help`: Structured diagnostic help output matching upstream rsync.
- `-a, --archive`: Archive mode (equivalent to `-rlptgoD`).
- `-r, --recursive`: Recurse into directories.
- `-l, --links`: Copy symlinks as symlinks.
- `-L, --copy-links`: Dereference symlinks into their referents.
- `-k, --copy-dirlinks`: Treat symlinked directories as directories.
- `-p, --perms`: Preserve permissions / mode.
- `-t, --times`: Preserve modification times (nanosecond precision).
- `-O, --omit-dir-times`: Omit directories when setting modification times.
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
- `--delete-before`, `--delete-during`, `--delete-delay`, `--delete-after`: Precise deletion timing.
- `--delete-excluded`: Delete excluded files from destination.
- `--max-delete=NUM`: Limit the maximum number of deleted files.
- `--existing`: Only update files that already exist on destination.
- `--ignore-existing`: Skip updating files that already exist on destination.
- `-m, --prune-empty-dirs`: Prune empty directory chains from file-list.
- `-f, --filter=RULE`: Rule-based filtering (`+`, `-`, `P`, `R`, `H`, `S`, `!`, `.`, `:`, `dir-merge`).
- `--exclude=PATTERN`, `--include=PATTERN`: Include/exclude pattern filters.
- `--exclude-from=FILE`, `--include-from=FILE`: Load rules from files.
- `-C, --cvs-exclude`: CVS exclusion rules and per-directory `.cvsignore` merge.
- `-F`: Shorthand for `:C` (dir-merge `.cvsignore`) / `-C`.
- `-b, --backup`: Make backups of updated or deleted files.
- `--backup-dir=DIR`: Relocate backups to designated directory hierarchy.
- `--suffix=SUFFIX`: Backup filename suffix (default `~`).
- `--delay-updates`: Stage updated files until the transfer pass is finished.
- `-T, --temp-dir=DIR`: Use temporary directory for staging during transfer.
- `--remove-source-files`: Remove source files after successful transfer.
- `--link-dest=DIR`, `--copy-dest=DIR`, `--compare-dest=DIR`: Alternate destination basis trees.
- `--safe-links`, `--copy-unsafe-links`: Path-escape link handling.
- `-i, --itemize-changes`: 11-character itemized change summary (`-ii` for unchanged items).
- `-v, --verbose`, `-q, --quiet`, `-n, --dry-run`.
