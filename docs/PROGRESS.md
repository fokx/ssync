# PROGRESS.md: Persistent Project Memory & Status

This document tracks implementation status across phases, records upstream test passing status, and maintains our active backlog and notes for future sessions.

---

## 1. Phase Checklist

- [x] **Phase 0: Deep Study, Planning, Decisions & Scaffolding**
  - [x] Upstream rsync C source analysis (`docs/RSYNC_NOTES.md`).
  - [x] Complete testsuite analysis and classification (`docs/TESTSUITE_MAP.md`).
  - [x] Decisions recorded (`docs/DECISIONS.md`).
  - [x] Implementation roadmap finalized (`docs/PLAN.md`).
  - [x] Project conventions documented (`docs/CONVENTIONS.md`).
  - [x] Out-of-tree test helper build verified (`target/upstream-tooldir`).
  - [x] Baseline upstream test run verified against reference rsync binary.
  - [x] Cargo workspace initialized with compiling skeletons, formatting, clippy, CI workflow.
  - [x] Initial git commit created on clean branch.
- [x] **Phase 1: Local Recursive File Copy & Traversal**
  - [x] Implement `ssync-fs` path handling & directory walker (`crates/ssync-fs/src/walker.rs`, `path.rs`).
  - [x] Implement basic metadata reader/writer (mode, mtime with nanoseconds, symlinks, `apply_to` in `metadata.rs`).
  - [x] Implement `ssync-cli` argument parser matching rsync subset (`-a, -r, -l, -L, -k, -K, -p, -t, -o, -g, -D, -d, -v, -q, -n, -R, --no-implied-dirs, -H, --delete/--del, --existing, -i, --include, --exclude, --copy-unsafe-links, --safe-links`).
  - [x] Pass target upstream tests: `hands`, `dirs`, `links`, `trimslash`, `relative` (plus `relative-content`, `relative-implied`, `symlink-ignore`, `longdir`, `unsafe-links`, `safe-links`, `safe-links-absolute-intree`, `safe-links-unsafe-def`, `duplicates`, `clean-fname-underflow`, `skiplist-spec`).
- [x] **Phase 2: Delta-Transfer Algorithm & Checksum Engine**
  - [x] Implement `Checksum1` rolling checksum in `ssync-core` (`crates/ssync-core/src/checksum.rs`).
  - [x] Implement strong hash abstraction (MD5, MD4, XXH64, SHA1) with seed support and whole-file checksumming.
  - [x] Implement block search hash table, collision chain capping (`MAX_CHAIN_LEN = 1024`), duplicate chain unlinking (issue #1115), and 4-offset collision skip heuristic (`crates/ssync-core/src/delta.rs`).
  - [x] Implement delta token generator and full reconstructor with property-based proptest roundtrip verification.
  - [x] Integrate quick-check comparison (`-c, --checksum`, `-u, --update`, `-I, --ignore-times`, `--size-only`, `--modify-window`), `--max-size`, `--min-size`, and staging modes (`--inplace`, `--append`, `--append-verify`, temporary file staging).
  - [x] Pass target upstream tests: `compare`, `update`, `append`, `inplace`, `hashsearch-chain`, `hashsearch-collision-skip`, `hashsearch-dup-chain`, `size-filter`.
- [x] **Phase 3: Filters, Deletion & Advanced Local Synchronization**
  - [x] Implement filter rule parser and wildmatch engine (`*`, `**`, `***`, `/`, character classes, negate, prefix modifiers, dir-merge, `.cvsignore`).
  - [x] Implement deletion logic (`--delete`, `--delete-before`, `--delete-during`, `--delete-delay`, `--delete-after`, `--delete-excluded`, `--max-delete`).
  - [x] Implement staging modes (`--delay-updates`, `-b/--backup`, `--backup-dir`, `--suffix`, `-T/--temp-dir`, `--existing`, `--ignore-existing`, `--omit-dir-times`).
  - [x] Implement hardlink preservation (`-H, --hard-links`) and alternate basis trees (`--link-dest`, `--copy-dest`, `--compare-dest`).
  - [x] Implement itemize changes (`-i, -ii, --itemize-changes`) with exact 11-character formatting, and `--stats`.
  - [x] Pass target upstream tests: `exclude`, `cvs-exclude`, `delete`, `delete-deep`, `delay-updates`, `backup`, `backup-deep`, `backup-dir-relative`, `hardlinks-deep`, `itemize`, `alt-dest-deep`, `temp-dir`.
- [ ] **Phase 4: Remote Shell Execution & Wire Protocol 31**
  - [ ] Implement multiplexed framing (`MPLEX_BASE`) and varint encoding in `ssync-proto`.
  - [ ] Implement flist wire serialization.
  - [ ] Implement `--server` / `--sender` mode dispatcher.
  - [ ] Verify two-way interoperability over SSH/pipe against upstream rsync binary.
- [ ] **Phase 5: Performance Optimization, Hardening & Security Audit**
  - [ ] SIMD acceleration for rolling checksum.
  - [ ] TOCTOU symlink race hardening.
  - [ ] Differential testing sweeps with `abdiff.py`.

---

## 2. Upstream Test Status Table

| Test Name | Classification | Current Status | Passing Phase | Notes |
|-----------|----------------|----------------|---------------|-------|
| `hands` | Required | **Passed** | Phase 1 | Canonical recursive tree transfer |
| `dirs` | Required | **Passed** | Phase 1 | Non-recursive directory transfer (`-d`) |
| `links` | Required | **Passed** | Phase 1 | Symlink preservation (`-l, -L, -k`) |
| `trimslash` | Required | **Passed** | Phase 1 | Trailing slash path semantics |
| `relative` | Required | **Passed** | Phase 1 | Relative and implied directory paths (`-R`) |
| `relative-content` | Required | **Passed** | Phase 1 | Relative path content delivery |
| `relative-implied` | Required | **Passed** | Phase 1 | Implied directories and `--no-implied-dirs` |
| `symlink-ignore` | Required | **Passed** | Phase 1 | Default behavior ignoring symlinks without `-l/-L/-a` |
| `longdir` | Required | **Passed** | Phase 1 | Deep nested directory traversal |
| `unsafe-links` | Required | **Passed** | Phase 1 | Unsafe link policy & `--copy-unsafe-links` |
| `safe-links` | Required | **Passed** | Phase 1 | Dropping unsafe escaping links under `--safe-links` |
| `safe-links-absolute-intree` | Required | **Passed** | Phase 1 | Absolute symlinks dropped under `--safe-links` |
| `safe-links-unsafe-def` | Required | **Passed** | Phase 1 | Validation of unsafe symlink definitions |
| `duplicates` | Required | **Passed** | Phase 1 | Deduplication of duplicate command line sources |
| `clean-fname-underflow` | Required | **Passed** | Phase 1 | Path component sanitization without underflow |
| `skiplist-spec` | Required | **Passed** | Phase 1 | Harness skip list parsing |
| `compare` | Required | **Passed** | Phase 2 | Checksum comparison and skip rules (`-c, -I, --size-only, --modify-window`) |
| `update` | Required | **Passed** | Phase 2 | Update-only semantics (`-u`) and directory replacement (`--force`) |
| `append` | Required | **Passed** | Phase 2 | In-place append verification (`--append`, `--append-verify`) |
| `inplace` | Required | **Passed** | Phase 2 | In-place block updating preserving inode |
| `hashsearch-chain` | Required | **Passed** | Phase 2 | Bounded hash table collision search with chain cap |
| `hashsearch-collision-skip` | Required | **Passed** | Phase 2 | Collision flood skip boundary preservation and statistics |
| `hashsearch-dup-chain` | Required | **Passed** | Phase 2 | Duplicate colliding chain unlinking (issue #1115) |
| `size-filter` | Required | **Passed** | Phase 2 | Size threshold filtering (`--max-size`, `--min-size`) |
| `exclude` | Required | **Passed** | Phase 3 | Exclude and filter rule evaluation |
| `cvs-exclude` | Required | **Passed** | Phase 3 | CVS ignore defaults, env vars, and deep `.cvsignore` scoping |
| `delete` | Required | **Passed** | Phase 3 | Deletion modes (`--delete-before/during/delay/after`) |
| `delete-deep` | Required | **Passed** | Phase 3 | Deletion at depth, `--max-delete`, `--existing`, `--ignore-existing` |
| `delay-updates` | Required | **Passed** | Phase 3 | Staging updates in `.~tmp~` and final atomic rename |
| `backup` | Required | **Passed** | Phase 3 | File backup semantics with suffix and backup dir |
| `backup-deep` | Required | **Passed** | Phase 3 | Recursive backup directory preservation |
| `backup-dir-relative` | Required | **Passed** | Phase 3 | Relative backup directory resolution |
| `hardlinks-deep` | Required | **Passed** | Phase 3 | Local hard link preservation across directory boundaries |
| `itemize` | Required | **Passed** | Phase 3 | Itemized changes output (`-i`, `-ii`) and verbose formatting |
| `alt-dest-deep` | Required | **Passed** | Phase 3 | `--link-dest`, `--copy-dest`, `--compare-dest` at depth |
| `temp-dir` | Required | **Passed** | Phase 3 | Cross-directory staging and missing temp dir failure |
| `00-hello` | Deferred | Pending | Phase 4 | Remote server smoke test |

*(Full mapping of all 374 upstream tests is maintained in `docs/TESTSUITE_MAP.md`)*

---

## 3. Backlog

- [ ] Investigate `fallocate` / `posix_fallocate` support on Linux for preallocating destination files.
- [ ] Benchmark AVX2 / NEON vectorization for `get_checksum1` rolling updates.
- [ ] Consider adding a `--json` output flag (in addition to standard rsync output) for machine readability.
- [ ] Evaluate fuzz testing harnesses for the filter rule and wildmatch parser.

---

## 4. Notes for Next Session (Phase 4: Remote Shell Execution & Wire Protocol 31)

1. **Immediate Focus**: Implement Phase 4: Remote Shell Execution & Wire Protocol 31.
2. **Key Components to Implement**:
   - `crates/ssync-proto`:
     - Protocol 31 wire handshake and compatibility bitflags (`CF_INC_RECURSE`, `CF_SYMLINK_TIMES`, `CF_SAFE_FLIST`, `CF_VARINT_FLIST_FLAGS`).
     - Multiplexed channel framing (`MPLEX_BASE = 7`, `MSG_DATA`, `MSG_INFO`, `MSG_ERROR_XFER`, etc.).
     - Variable-length integer encoding and decoding (`varint`, `varlong`).
     - File list (`flist`) wire serialization with prefix compression and metadata encoding.
   - `crates/ssync-cli`:
     - Server command dispatcher (`--server`, `--sender`).
     - Remote shell launcher (`ssh`, `-e, --rsh`, or `support/lsh.sh`).
     - Two-way pipe/socket transport loop between local generator/receiver and remote server.
3. **Verification**:
   - Unit and integration tests for multiplexing, varint wire encoding, and flist serialization.
   - Run upstream testsuite targets for Phase 4: `00-hello`, `alt-dest`, `ssh-basic`, `hardlinks`.
