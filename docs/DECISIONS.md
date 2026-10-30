# DECISIONS.md: Architectural & Design Decisions for ssync

This document records the foundational technical and architectural decisions for **ssync**, an independent, clean-room reimplementation of rsync in Rust.

---

## 1. Licensing Policy

* **Decision**: **ssync is licensed under Apache-2.0**.
* **Upstream Relationship**: The reference rsync source repository (`/f/rsync`, GPL-3.0-or-later) is strictly read-only and consulted solely for behavioral facts, protocol specifications, data layouts, and wire formats. No upstream C code, comments, or structure are copied or mechanically translated into our repository. Upstream test files (`testsuite/*_test.py`) remain in `/f/rsync/testsuite` and are NOT copied into our repository; our test runner invokes them in place against our binary.
* **Justification**: Apache-2.0 provides explicit patent grant protections while remaining permissive and widely adoptable across open-source tools and commercial systems without copyleft encumbrance, honoring the clean-room nature of the implementation.

---

## 2. Wire & Protocol Compatibility Strategy

* **Decision**: **Phased approach starting with an idiomatic local delta engine, followed by standard rsync Protocol 31/30 wire compatibility**.
* **Details**:
  1. **Phase 1 to Phase 3**: Build a self-contained local synchronization engine implementing the delta-transfer algorithm (weak rolling checksum + strong digest), file list walking, attribute management, and exclude filters.
  2. **Phase 4 & Beyond**: Implement wire protocol 31 (with protocol 30 fallback) over standard I/O streams (`std::io::Read` / `std::io::Write`). This supports:
     - Remote shell mode (`ssh` / `lsh.sh` launching `ssync --server`).
     - Protocol 31 multiplexed packet framing (`MPLEX_BASE`), variable-length integers (`varint` / `varlong`), flist wire representation, and message channels.
* **Justification**: Protocol 31 is the stable standard used by all modern rsync releases (3.1.x – 3.4.x). Starting with a clean, decoupled local transfer engine guarantees that the core delta math, filesystem traversal, and metadata logic are completely robust and unit-tested before adding wire framing and multi-process state machines.

---

## 3. Crate Layout (Cargo Workspace)

* **Decision**: A Cargo workspace organized into focused, modular crates:
  - `crates/ssync-core`: Core algorithms and data structures:
    - Rolling weak checksum (`Checksum1`: Adler32-variant with `CHAR_OFFSET=0`).
    - Strong digests (`md4`, `md5`, `xxhash`, `sha1` abstractions).
    - Block matching engine (hash table / tag table, chunk-sliding window, token generator).
  - `crates/ssync-fs`: Local filesystem operations:
    - Path normalization and directory traversal (`walkdir` / custom iterator).
    - Metadata extraction and preservation (mtime, atime, mode/permissions, uid/gid, symlinks, hardlink tracking).
    - Atomic temp-file staging, rename semantics, and path safety validation.
    - Exclude / include filter rule engine and glob wildmatching.
  - `crates/ssync-proto`: Wire protocol serialization and multiplexing:
    - Protocol negotiation and compatibility flags (`compat_flags`).
    - Varint / varlong integer encoders and decoders.
    - Multiplexed channel framing (`MPLEX_BASE`, `MSG_DATA`, `MSG_INFO`, `MSG_ERROR_*`).
    - File list (`flist`) wire encoding and decoding.
  - `crates/ssync-cli`: Binary entry point (`ssync`):
    - Command-line parsing with flag compatibility matching `rsync`.
    - Local and remote execution orchestration.
    - Exit code mapping to standard `RERR_*` codes.
* **Justification**: Separation of concerns allows testing core delta matching and filter logic without spinning up processes or dealing with I/O streams. `ssync-core` and `ssync-fs` can be consumed independently as libraries.

---

## 4. Key Dependencies

We strictly prefer few, well-maintained, battle-tested crates with minimal transitive dependencies:
* **CLI Parsing**: `clap` (v4 with `derive` feature). Allows expressive definition of flags, exact aliases, value parsers, and custom argument grouping to reproduce rsync's CLI interface.
* **Hashing & Checksums**:
  - `xxhash-rust` (xxh64, xxh3): Extremely fast, modern non-cryptographic hash for delta matching.
  - `md-5`: Standard MD5 implementation for protocol 30/31 compatibility.
  - `md4`: Standard MD4 implementation for legacy rsync compatibility.
  - `sha1`: SHA-1 implementation.
* **Error Handling**: `thiserror` for clean typed error hierarchies in library crates; `anyhow` for CLI context bubbling.
* **Filesystem & System APIs**: `nix` and `libc` for POSIX metadata, `utimensat`, `fchmod`, `lchown`, and file descriptors.
* **Concurrency / I/O**: **Synchronous (blocking) I/O with standard POSIX threads / streaming**.
  - *Rationale*: Rsync's protocol is inherently sequential token-and-block streaming over point-to-point pipes or sockets. Async runtimes (such as Tokio) introduce substantial overhead, memory fragmentation, and complexity without benefit for streaming point-to-point file synchronization.
* **Testing**:
  - `proptest`: Property-based testing for the rolling checksum and delta reconstruction.
  - `tempfile`: Isolated scratch directories for integration tests.
  - `assert_cmd`: CLI invocation assertions.

---

## 5. Target Platforms & MSRV

* **Target Platforms**:
  - Tier 1: Linux (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`).
  - Tier 2: macOS (`x86_64-apple-darwin`, `aarch64-apple-darwin`), FreeBSD.
* **MSRV**: **Rust 1.85.0** (or modern stable, 2024 edition compatible).

---

## 6. Scope Boundaries for v1

### In Scope for v1:
* **Core Flag Compatibility**:
  - `-a, --archive` (and components: `-r, --recursive`, `-l, --links`, `-p, --perms`, `-t, --times`, `-o, --owner`, `-g, --group`, `-D` [partially: symlinks, fifos]).
  - `-v, --verbose`, `-q, --quiet`.
  - `-n, --dry-run`.
  - `-c, --checksum` (skip-check via full file checksum).
  - `-u, --update` (skip files that are newer on destination).
  - `-d, --dirs` (transfer directories without recursing).
  - `-m, --prune-empty-dirs`.
  - `-I, --ignore-times`, `--size-only`, `--modify-window`.
  - `-i, --itemize-changes` (full 11-character change format).
  - `--delete`, `--delete-before`, `--delete-during`, `--delete-after`, `--delete-excluded`, `--max-delete`.
  - `--existing`, `--ignore-existing`.
  - `--partial`, `--partial-dir`, `--delay-updates`.
  - `--inplace`, `--append`, `--append-verify`.
  - `-b, --backup`, `--backup-dir`, `--suffix`.
  - `--compare-dest`, `--copy-dest`, `--link-dest` (`-H, --hard-links`).
  - `-T, --temp-dir`.
  - `-S, --sparse`.
  - `-z, --compress` (zlib / zstd streaming tokens).
  - Filtering: `-f, --filter`, `--exclude`, `--include`, `--exclude-from`, `--include-from`, `-C, --cvs-exclude`.
  - Output / reporting: `--stats`, `--progress`, `--human-readable` (`-h`).
  - Path styles: trailing slashes (`src/` vs `src`), `--relative` (`-R`), `--no-implied-dirs`.

### Deferred to Post-v1:
* Full daemon mode (`--daemon`, `rsyncd.conf` parser, multi-tenant authentication, chroot inner modules).
* Advanced security TOCTOU race loop handlers (`openat2(RESOLVE_BENEATH)` complex fd caching).
* Batch mode (`--write-batch`, `--read-batch`).
* Extended attributes (`-X, --xattrs`) and POSIX ACLs (`-A, --acls`).
* Character set conversion (`--iconv`).
* IPv6/DNS specific daemon lookup controls.
