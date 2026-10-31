# PLAN.md: Phased Implementation Roadmap for ssync

This document establishes the step-by-step implementation roadmap for building **ssync**. Each phase is scoped to a single focused development session, ending in a working, verifiable, and comprehensively tested state.

---

## Phase Overview

```
Phase 0: Research, Decisions & Project Scaffolding (Current Session)
   │
   ▼
  Phase 1: Local Recursive File Copy & Traversal
   │  - Directory walking, path semantics (trailing slashes), permissions, mtime, symlinks
   │  - CLI flags: -r, -l, -p, -t, -a, -d, -n, -v, -q
   │
   ▼
Phase 2: Delta-Transfer Algorithm & Checksum Engine
   │  - Rolling Checksum1 (Adler32-variant), strong digests (MD5, XXH64, SHA1)
   │  - Block size heuristics, sliding window matching, literal/token streaming
   │  - Quick-check skips, -c/--checksum, -u/--update, --inplace, --append
   │
   ▼
Phase 3: Filters, Deletion & Advanced Local Synchronization
   │  - Exclude/include rules, glob/wildmatching, .cvsignore, merge files
   │  - Deletion engine (--delete, --delete-before/during/delay/after, --max-delete)
   │  - Staging & atomic updates: --delay-updates, --partial, -b/--backup, -H/--hard-links
   │  - Itemize changes (-i) and transfer statistics (--stats)
   │
   ▼
Phase 4: Remote Shell Execution & Wire Protocol 31
   │  - Server mode (`--server`, `--sender`), SSH invocation (`-e`)
   │  - Protocol 31 multiplexed channels (`MPLEX_BASE`), varint encoding, flist wire stream
   │  - Interoperability against upstream rsync binary over pipe/SSH
   │
   ▼
Phase 5: Performance Optimization, Hardening & Security Audit
   │  - SIMD rolling checksum acceleration (AVX2 / NEON)
   │  - Path traversal & symlink race hardening (secure open/rename)
   │  - Differential fuzzing via abdiff.py
```

---

## Phase Details

### Phase 1: Local Recursive File Copy & Traversal
- **Goal**: Implement a robust local file copying engine supporting recursive directory trees, exact trailing-slash semantics, and basic metadata preservation.
- **Deliverables**:
  - `ssync-fs`: Directory scanner, file metadata reader/writer (`mtime`, `mode`, `uid`/`gid`, symlinks).
  - `ssync-cli`: CLI argument parser for `-r`, `-l`, `-p`, `-t`, `-o`, `-g`, `-a`, `-d`, `-n`, `-v`, `-q`, and positional path arguments.
  - Path normalization: Strict handling of `src` vs `src/`, relative paths, and clean filenames.
- **Acceptance Criteria**:
  - Recursively synchronizes nested directories and files on local storage.
  - Matches rsync exit codes for missing source files and syntax errors.
  - Preserves permissions and timestamps within filesystem resolution.
  - Passes unit tests and integration tests.
- **Target Upstream Tests**:
  - `hands`: Canonical end-to-end directory sync test.
  - `dirs`: Directory handling without recursion (`-d`).
  - `links`: Symlink creation and preservation (`-l`).
  - `trimslash`: Trailing slash normalization.
  - `relative`: Implied and relative directory handling (`-R`).
- **Risks**:
  - Subtle path joining / trailing slash edge cases when copying into an existing destination directory.

---

### Phase 2: Delta-Transfer Algorithm & Checksum Engine
- **Goal**: Implement the core rsync delta-transfer algorithm in isolation and integrate it into the local file transfer engine.
- **Deliverables**:
  - `ssync-core`:
    - `Checksum1` rolling checksum with step-by-step window update.
    - Strong digests (MD5, XXH64, SHA1, MD4).
    - Block matching hash table (`hash_table[SUM2HASH(sum1)]`) with collision chain cap (`MAX_CHAIN_LEN = 1024`).
    - Sliding window token/literal generator.
    - Reconstructor copying matched basis blocks and literal bytes.
  - `ssync-cli`: Support for `-c, --checksum`, `-u, --update`, `-I, --ignore-times`, `--size-only`, `--modify-window`, `--inplace`, `--append`, `--append-verify`.
- **Acceptance Criteria**:
  - Property tests verify that reconstructing any file from an arbitrary basis + delta yields identical bytes and whole-file checksum.
  - Transfers only modified blocks when updating large files.
  - Unchanged files are skipped instantly according to quick-check heuristics.
- **Target Upstream Tests**:
  - `compare`: Validation of `-c`, `-I`, `--size-only`, `--modify-window`.
  - `update`: Update-only semantics (`-u`) preserving newer destination files.
  - `append`: In-place appending (`--append`, `--append-verify`).
  - `inplace`: In-place modification without temporary staging file (`--inplace`).
  - `hashsearch-chain`, `hashsearch-collision-skip`: Bounded hash table collision search.
- **Risks**:
  - Off-by-one errors in sliding window boundaries or remaining partial blocks at end-of-file.

---

### Phase 3: Filters, Deletion & Advanced Local Synchronization
- **Goal**: Complete the local feature set with rule filtering, deletion strategies, atomic staging, hardlinks, and detailed reporting.
- **Deliverables**:
  - Filter rule parser and wildmatch pattern matcher (`*`, `**`, `***`, `/`, `!`, `dir-merge`, `-C`).
  - Deletion engine: `--delete`, `--delete-before`, `--delete-during`, `--delete-after`, `--delete-excluded`, `--max-delete`.
  - Staging & atomic updates: `--delay-updates` (staging in `.~tmp~`), `--partial`, `--partial-dir`, `-b/--backup`, `--backup-dir`, `--suffix`.
  - Hardlink preservation (`-H, --hard-links`) tracking `(dev, ino)`.
  - Sparse file writing (`-S, --sparse`) utilizing `SEEK_HOLE`/`SEEK_DATA` or zero blocks.
  - Itemize changes format (`-i, --itemize-changes`) matching the exact 11-character output, and `--stats`.
- **Acceptance Criteria**:
  - Exclude and include patterns match rsync behavior across nested directory trees.
  - Deletion removes obsolete files at the configured stage without exceeding `--max-delete`.
  - Hard links are preserved with identical target inodes on destination.
  - Itemize string matches rsync output character-for-character.
- **Target Upstream Tests**:
  - `exclude`, `filter-depth`, `cvs-exclude`, `merge`.
  - `delete`, `delete-deep`.
  - `delay-updates`, `delay-updates-deep`, `partial`.
  - `backup`, `backup-deep`, `backup-dir-relative`.
  - `hardlinks`, `hardlinks-deep`.
  - `itemize`.
- **Risks**:
  - Complex precedence ordering in per-directory filter rules (`dir-merge`).

---

### Phase 4: Remote Shell Execution & Wire Protocol 31
- **Goal**: Implement rsync Protocol 31 wire communication over standard I/O streams, enabling remote synchronization over SSH.
- **Deliverables**:
  - `ssync-proto`:
    - Protocol 31 handshake and compatibility flags (`compat_flags`).
    - Multiplexed channel encoder/decoder (`MPLEX_BASE`).
    - Variable-length integer encoding (`varint`, `varlong`).
    - File list (`flist`) wire serialization with prefix compression.
  - `ssync-cli`:
    - Remote shell spawning (`ssh` or `support/lsh.sh`) with remote command `-e`.
    - Server mode dispatcher (`--server`, `--sender`).
- **Acceptance Criteria**:
  - `ssync` client successfully synchronizes against `rsync --server` over SSH/pipe.
  - `rsync` client successfully synchronizes against `ssync --server` over SSH/pipe.
- **Target Upstream Tests**:
  - `00-hello`, `ssh-basic`, `alt-dest`, `max-alloc-zero`, `remote-shell-newline-escaping`, `scanner-argv-bounds`.
  - Upstream tests exercising `lsh.sh`.
- **Risks**:
  - Deadlocks in multiplexed full-duplex pipe I/O if channel buffers fill without being drained.

---

### Phase 5: Performance Optimization, Hardening & Security Audit
- **Goal**: Accelerate core transfer loops and harden against adversarial filesystem layouts and TOCTOU races.
- **Deliverables**:
  - SIMD rolling checksum acceleration using `std::simd` or explicit AVX2 / ARM NEON intrinsics.
  - Path traversal and symlink race defenses (`O_NOFOLLOW`, `RESOLVE_BENEATH` / `openat2`).
  - Differential testing against reference rsync using `testsuite/abdiff.py`.
- **Acceptance Criteria**:
  - Benchmarks show rolling checksum and delta generation throughput matching or exceeding upstream rsync.
  - Clean run on `abdiff.py` differential test sweeps.
