# ssync

A fast, memory-safe, clean-room reimplementation of `rsync` in Rust.

[![Crates.io](https://img.shields.io/crates/v/ssync.svg)](https://crates.io/crates/ssync)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![CI](https://github.com/ssync/ssync/actions/workflows/ci.yml/badge.svg)](https://github.com/ssync/ssync/actions)
[![MSRV: 1.85.0](https://img.shields.io/badge/MSRV-1.85.0-informational.svg)](https://blog.rust-lang.org/)

---

## Overview

`ssync` is a modern, high-performance file transfer and synchronization tool. It faithfully implements the `rsync` delta-transfer algorithm, wire protocol 31, and common command-line flags while providing strict memory safety guarantees and robust security against symlink-based attacks.

### Key Goals & Highlights

- **Algorithmically faithful**: Full implementation of the rsync delta-transfer algorithm (weak rolling checksum + strong hash block matching, chunk-sliding window, tokenized reconstructor).
- **Flag compatible**: Drop-in compatible CLI flag subset matching standard `rsync`, verified against upstream rsync test suites.
- **Wire Protocol 31**: Fully interoperable with upstream `rsync` over SSH or daemon mode via standard I/O multiplexing.
- **Modern & Safe**: Built with modern Rust (Edition 2024, MSRV 1.85.0), strict safety invariants (`#![forbid(unsafe_code)]` in safe crates), and permissive Apache-2.0 licensing.
- **SIMD Accelerated**: AVX2-accelerated and SWAR unrolled rolling checksum engines for blazing fast delta scans.
- **Race Hardened**: Directory confinement and symlink race mitigation using held directory file descriptors (`dirfd`).

---

## Installation

### From crates.io

```bash
cargo install ssync
```

### From Source

```bash
git clone https://github.com/ssync/ssync.git
cd ssync
cargo install --path crates/ssync-cli
```

---

## Quickstart

### Local Synchronization

```bash
# Basic recursive copy preserving metadata (archive mode) with verbose progress
ssync -av /path/to/source/ /path/to/dest/

# Itemized changes output (-i) with delete extraneous destination files
ssync -avi --delete /path/to/source/ /path/to/dest/

# Exclude patterns and ignore rules
ssync -av --exclude="*.tmp" --exclude="target/" /path/to/source/ /path/to/dest/
```

### Remote Synchronization over SSH

```bash
# Push local directory to remote host
ssync -avz /local/path/ user@remote.host:/remote/path/

# Pull remote directory to local destination
ssync -avz user@remote.host:/remote/path/ /local/path/

# Custom remote shell or port
ssync -avz -e "ssh -p 2222" /local/path/ user@remote.host:/remote/path/
```

---

## Architecture & Workspace Crates

`ssync` is engineered as a modular Cargo workspace:

| Crate | Crates.io | Description |
|:------|:----------|:------------|
| [`ssync`](crates/ssync-cli) | [![Crates.io](https://img.shields.io/crates/v/ssync.svg)](https://crates.io/crates/ssync) | CLI application binary, argument parser, and orchestrator |
| [`ssync-core`](crates/ssync-core) | [![Crates.io](https://img.shields.io/crates/v/ssync-core.svg)](https://crates.io/crates/ssync-core) | Rolling checksums (AVX2/SIMD), digests (MD4, MD5, XXH, SHA1), and delta engine |
| [`ssync-fs`](crates/ssync-fs) | [![Crates.io](https://img.shields.io/crates/v/ssync-fs.svg)](https://crates.io/crates/ssync-fs) | Directory traversal, metadata preservation, filters, and held-dirfd security |
| [`ssync-proto`](crates/ssync-proto) | [![Crates.io](https://img.shields.io/crates/v/ssync-proto.svg)](https://crates.io/crates/ssync-proto) | Wire protocol 31, varint encoders, packet multiplexing, and file list codecs |

---

## Security Invariants

`ssync` incorporates defense-in-depth protections against filesystem race conditions and traversal attacks:

- **Held-`dirfd` Isolation**: Paths are opened and traversed relative to parent file descriptors (`openat`, `fstatat`, `unlinkat`), preventing directory-swap TOCTOU vulnerabilities.
- **Symlink Confinement**: Symlink targets are strictly checked against destination boundaries (`--safe-links` and `--copy-unsafe-links`) to prevent arbitrary directory breakouts.
- **Sparse File Safety**: Sparse file hole-punching and detection avoid unnecessary I/O while preserving physical disk allocations.

---

## Supported Options

`ssync` supports the core options required for day-to-day syncing, backup scripts, and upstream interoperability:

- **Transfer Modes**: `-a, --archive` (`-rlptgoD`), `-r, --recursive`, `-d, --dirs`, `-R, --relative`, `--no-implied-dirs`.
- **Symlink Handling**: `-l, --links`, `-L, --copy-links`, `-k, --copy-dirlinks`, `--safe-links`, `--copy-unsafe-links`.
- **Metadata**: `-p, --perms`, `-t, --times`, `-O, --omit-dir-times`, `-o, --owner`, `-g, --group`, `-H, --hard-links`.
- **Heuristics & Checks**: `-c, --checksum`, `-u, --update`, `-I, --ignore-times`, `--size-only`, `--modify-window=N`.
- **Delta Transfer**: `--inplace`, `--append`, `--append-verify`, `--block-size=N`, `--stats`.
- **Deletion**: `--delete, --del`, `--delete-before`, `--delete-during`, `--delete-delay`, `--delete-after`, `--delete-excluded`, `--max-delete=NUM`.
- **Filtering**: `-f, --filter=RULE`, `--exclude=PATTERN`, `--include=PATTERN`, `--exclude-from=FILE`, `--include-from=FILE`, `-C, --cvs-exclude`, `-F`.
- **Backups & Staging**: `-b, --backup`, `--backup-dir=DIR`, `--suffix=SUFFIX`, `--delay-updates`, `-T, --temp-dir=DIR`, `--remove-source-files`.
- **Alternates**: `--link-dest=DIR`, `--copy-dest=DIR`, `--compare-dest=DIR`.
- **Remote & Protocol**: `[user@]host:path`, `-e, --rsh=CMD`, `--rsync-path=PROG`, `-s, --protect-args`, `--old-args`, `--server`, `--sender`, `-z, --compress`, `--max-alloc=SIZE`.
- **Output & Diagnostics**: `-v, --verbose`, `-q, --quiet`, `-n, --dry-run`, `-i, --itemize-changes`, `--info=help`, `--debug=help`.

---

## Building & Testing

```bash
# Build the workspace
cargo build --workspace

# Run all unit and integration tests
cargo test --workspace

# Run linter and formatting checks
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# Execute upstream rsync test suite against ssync binary
./scripts/run_upstream_tests.sh hands dirs links relative
```

---

## License

`ssync` is licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or [LICENSE-APACHE](LICENSE-APACHE)).

Upstream reference code (`/f/rsync`, GPL-3.0-or-later) is strictly consulted for behavioral specification and protocol verification; no upstream code or test scripts are copied into this repository. See [docs/DECISIONS.md](docs/DECISIONS.md) for licensing and architectural rationale.
