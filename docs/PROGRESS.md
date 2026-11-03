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
- [x] **Phase 4: Remote Shell Execution & Wire Protocol 31**
  - [x] Implement multiplexed framing (`MPLEX_BASE`), message codes, and varint/varlong encoding in `ssync-proto`.
  - [x] Implement protocol 31 handshake and compatibility exchange (`CF_SAFE_FLIST`, `CF_CHKSUM_SEED_FIX`).
  - [x] Implement flist wire serialization with prefix compression and metadata encoding.
  - [x] Implement block checksum and delta token streaming over the wire channel.
  - [x] Implement client remote shell execution (`[user@]host:path`, `$RSYNC_RSH`, `safe_arg` remote quoting, secluded args `-s`).
  - [x] Implement `--server` / `--sender` mode dispatcher over multiplexed stdio.
  - [x] Verify interoperability over SSH/pipe (`lsh.sh`) against upstream rsync binary and passing upstream test suite.
- [x] **Phase 5: Performance Optimization, Hardening & Security Audit**
  - [x] SIMD acceleration for rolling checksum (`calculate_avx2`, unrolled scalar fallback, runtime CPU detection, 5.77 GiB/s throughput).
  - [x] TOCTOU symlink race hardening (`safe_create_dir_all`, `safe_rename_into`, `safe_open_source` using held ancestor dirfds and `O_NOFOLLOW`).
  - [x] Differential testing sweeps with `abdiff.py` (`pathshape` 13/13 OK, `size` 14/14 OK, `options` 24 OK).

---

## 2. Upstream Test Status Table

### Full Suite Metrics

| Metric | Start of Session | End of Session | Delta |
|--------|------------------|----------------|-------|
| **Total Tests** | 374 | 374 | 0 |
| **Total Passed** | 148 | 159 | **+11** |
| **Total Failed** | 114 | 103 | **-11** |
| **Total Skipped** | 111 | 111 | 0 |
| **Total Expected Fail (XFAIL)** | 1 | 1 | 0 |
| **Required for v1 Passed** | 101 | 112 | **+11** |
| **Required for v1 Failed** | 11 | 0 | **-11 (100% PASS)** |
| **Required for v1 Skipped** | 9 | 9 | 0 |

### Skipped Required Tests Breakdown (9 tests)
- `acls-unpinnable` (SKIP): Requires POSIX ACL filesystem features not present on test tmpfs.
- `atimes` (SKIP): Requires kernel/mount support for atime recording.
- `crtimes` (SKIP): Requires filesystem birth time / creation time support (macOS/FreeBSD ext4/bflash).
- `link-dest-symlink-enotsup` (SKIP): Requires filesystem that rejects symlink creation with ENOTSUP.
- `macos-setgid-ordinary-mode-regression` (SKIP): macOS-specific setgid directory inheritance behavior.
- `open-noatime` (SKIP): Requires `O_NOATIME` filesystem permission/support.
- `partial-protected-regular-retry-policy` (SKIP): Advanced Linux protected_regular sysctl policy.
- `preallocate` (SKIP): Requires underlying filesystem fallocate support.
- `symlink-unix-perms` (SKIP): Unix symlink permissions support (platform dependent).

### Selected Upstream Tests Passing Status

| Test Name | Classification | Current Status | Passing Phase / Commit | Notes |
|-----------|----------------|----------------|------------------------|-------|
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
| `00-hello` | Deferred | **Passed** | Phase 4 | Remote server smoke test, info/debug help, secluded args |
| `ssh-basic` | Deferred | **Passed** | Phase 4 | Basic remote shell transfer and renamed file transfer with `--delete` |
| `alt-dest` | Deferred | **Passed** | Phase 4 | Alternate destination trees (`--compare-dest`, `--copy-dest`, `--link-dest`) locally and over `lsh.sh` |
| `max-alloc-zero` | Deferred | **Passed** | Phase 4 | Verbatim forwarding of `--max-alloc=0` across remote shell |
| `remote-shell-newline-escaping` | Deferred | **Passed** | Phase 4 | Remote shell argument quoting and newline escaping |
| `scanner-argv-bounds` | Deferred | **Passed** | Phase 4 | Robust argument scanner bounds against buffer overflows and option floods |
| `relative-mkpath-symlink` | Required | **Passed** | Phase 5 | Destination directory creation confinement with held dirfds |
| `relative-mkpath-dir-symlink` | Required | **Passed** | Phase 5 | Multi-component relative path confinement with `O_NOFOLLOW` |
| `rename-fullpath-symlink-race` | Required | **Passed** | Phase 5 | Atomic rename confined beneath transfer root using `renameat` |
| `symlink-race-source` | Required | **Passed** | Phase 5 | Source-side parent symlink TOCTOU confinement via pinned ancestor dirfds |
| `dest-symlinked-dir` | Required | **Passed** | Commit `235e893` | Follow operator-named symlinked destination directory |
| `output-options` | Required | **Passed** | Commit `6790952` | Support `--out-format`, `--list-only`, and summary stats block |
| `ki58-log-format-percent` | Required | **Passed** | Commit `6790952` | Support `%o` / `%i` formatting in log/out-format strings |
| `files-from-depth` | Required | **Passed** | Commit `f141217` | Support `--files-from` and relative path bounds |
| `files-from-path-clamp` | Required | **Passed** | Commit `f141217` | Strip leading slashes and clamp path components under `--files-from` |
| `rrsync-write-only-files-from` | Required | **Passed** | Commit `f141217` | Read-only / write-only confinement for `--files-from` under rrsync |
| `ki72-safe-links-backup` | Required | **Passed** | Commit `f2bffad` | Preserve symlinks in backups while enforcing `--safe-links` policy |
| `fuzzy` | Required | **Passed** | Commit `c8eb05d` | Fuzzy basis candidate selection based on filename and size heuristics |
| `fuzzy-basis` | Required | **Passed** | Commit `c8eb05d` | Fuzzy basis delta transfer matching nearest identical blocks |
| `filter-depth` | Required | **Passed** | Commit `5022554` | Enforce 32-level merge-file recursion cap and clean depth-limit error (code 11) |
| `filter-merge-recursion` | Required | **Passed** | Commit `5022554` | Prevent unbounded merge-file loops |
| `filter-merge-content-echo` | Required | **Passed** | Commit `5022554` | Redact rule file contents in diagnostics; sender filter matching tracing |
| `scanner-delete-delay-overread` | Required | **Passed** | Commit `0d15483` | Support `--super` and `--no-super` flags |
| `protected-regular` | Required | **Passed** | Commit `55a997b` | Retry inplace open without `O_CREAT` on EACCES (Linux `fs.protected_regular`) |
| `rrsync-backup-dir-inband-pivot` | Required | **Passed** | Commit `85add65` | Forward client options, execute server-side backups and symlink directory replacement |
| `rrsync-no-overwrite-partial-dir` | Required | **Passed** | Commit `85add65` | Prevent partial directory overwrite in wire transfers |
| `rrsync-no-overwrite-logfile` | Required | **Passed** | Commit `85add65` | Accept `--log-format` alias and protect logfiles |
| `rrsync-receiver-file-arg` | Required | **Passed** | Commit `ae0902f` | Single-file destination pathing under pinned wire transfers |
| `rrsync-root-relative-paths` | Required | **Passed** | Commit `ae0902f` | Wire server `--link-dest` basis hardlinking and root-relative paths |
| `rrsync-pull-arg-shapes` | Required | **Passed** | Commit `ae0902f` | Trailing slash symlink directory dereference and wire FIFO encoding |
| `append-abort-mid-write` | Required | **Passed** | Commit `fbe4340` | Buffer writes in 256KB chunks and avoid premature truncation in inplace mode |
| `growing-file` | Required | **Passed** | Commit `c61c9fd` | Enforce `--bwlimit` rate limiting and ignore mid-transfer file growth |
| `source-change-size-continues` | Required | **Passed** | Commit `c61c9fd` | Diagnose and skip mid-transfer diminished files cleanly |
| `backup-crossdev-copy` | Required | **Passed** | Commit `c718e71` | Recreate symlinks, FIFOs, and special devices across devices on backup |
| `partial` | Required | **Passed** | Commit `0f7242f` | Support `--partial` and `--partial-dir` staging, resumption, and cleanup |
| `partial-protected-regular-retry-linux` | Required | **Passed** | Commit `0f7242f` | Resume partial files under `fs.protected_regular` sysctl |
| `rrsync-files-from-stdin` | Required | **Passed** | Commit `6793568` | Stream null-terminated `--files-from=-` list over wire protocol |
| `rrsync-merge-file-confine` | Required | **Passed** | Commit `6b66cfe` | Confine merge files and forward client filter rules over wire |
| `rrsync-no-overwrite-delay-symlinks` | Required | **Passed** | Commit `44507b4` | Delay symlink creation until end of transfer (`--delay-symlinks`) |
| `rrsync-no-overwrite-backup-collision` | Required | **Passed** | Commit `44507b4` | Preserve backup collision files on deletion under `--ignore-existing` |
| `rrsync-sender-leaf-flip` | Required | **Passed** | Commit `32dddee` | Open sender sources with `O_NOFOLLOW` and signal `MsgCode::NoSend` on raced symlink flips |
| `backup` | Required | **Passed** | Commit `11ca0b4` | Copy rather than hard-link files during inplace backups |

*(Full mapping of all 374 upstream tests is maintained in `docs/TESTSUITE_MAP.md`)*

---

## 3. Backlog: Remaining Required Failures (0 tests)

**All 112 non-skipped Required-for-v1 upstream tests pass!**
The remaining 9 tests in the Required category are skipped due to kernel/filesystem environment preconditions:
- `acls-unpinnable` (SKIP): Requires POSIX ACL filesystem features not present on test tmpfs.
- `atimes` (SKIP): Requires kernel/mount support for atime recording.
- `crtimes` (SKIP): Requires filesystem birth time / creation time support (macOS/FreeBSD ext4/bflash).
- `link-dest-symlink-enotsup` (SKIP): Requires filesystem that rejects symlink creation with ENOTSUP.
- `macos-setgid-ordinary-mode-regression` (SKIP): macOS-specific setgid directory inheritance behavior.
- `open-noatime` (SKIP): Requires `O_NOATIME` filesystem permission/support.
- `partial-protected-regular-retry-policy` (SKIP): Advanced Linux protected_regular sysctl policy.
- `preallocate` (SKIP): Requires underlying filesystem fallocate support.
- `symlink-unix-perms` (SKIP): Unix symlink permissions support (platform dependent).

---

## 4. Notes for Next Session

1. **Session Achievements**:
   - Resolved all 11 remaining Required failures from the backlog plus 1 regression fix (`backup`), reaching **100% pass rate on all runnable Required-for-v1 tests** (112/112 passed, 9/9 skipped due to filesystem/kernel capability).
   - Required for v1 passing tests increased from **101 to 112** (+11 passing tests, 0 failing).
   - Total upstream test suite pass count increased from **148 to 159** (+11 passing tests).
   - Zero test regressions across the entire suite.
   - All 50 Rust unit and integration tests passing (`cargo test`).
   - Codebase is 100% clean: `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` produce zero warnings or errors.
   - Upstream reference repository `/f/rsync` confirmed strictly unmodified.

2. **Root Causes Fixed This Session**:
   - **Commit `fbe4340`**: Buffered writes in 256KB chunks and avoided premature truncation in inplace mode (fixed `append-abort-mid-write`).
   - **Commit `c61c9fd`**: Enforced `--bwlimit` rate limiting and diagnosed mid-transfer diminished files (fixed `growing-file`, `source-change-size-continues`).
   - **Commit `c718e71`**: Recreated symlinks, FIFOs, and special devices on cross-device backup (fixed `backup-crossdev-copy`).
   - **Commit `0f7242f`**: Supported `--partial` and `--partial-dir` staging, resume, and foreign symlink policy (fixed `partial`, `partial-protected-regular-retry-linux`).
   - **Commit `6793568`**: Streamed and received null-terminated files-from list over wire protocol (fixed `rrsync-files-from-stdin`).
   - **Commit `6b66cfe`**: Confined merge file opens and forwarded client filter rules over wire (fixed `rrsync-merge-file-confine`).
   - **Commit `44507b4`**: Delayed symlink creation and preserved backup collisions with ignore-existing (fixed `rrsync-no-overwrite-delay-symlinks`, `rrsync-no-overwrite-backup-collision`).
   - **Commit `32dddee`**: Opened wire sender sources with `O_NOFOLLOW` and signaled `MsgCode::NoSend` on raced symlink flips (fixed `rrsync-sender-leaf-flip`).
   - **Commit `11ca0b4`**: Copied rather than hard-linked files during inplace backups (fixed `backup`).

3. **Recommended Next Steps**:
   - Begin tackling Deferred tests that are now within reach:
     - `delay-symlinks` and `delay-symlinks-remote`: Validate the newly implemented `--delay-symlinks` wire/local flag against standalone tests.
     - `files-from`: Validate edge cases with `--files-from` against remote servers.
     - `symlink-dirlink-basis`: Verify alternate basis directories with symlinked directory targets.
     - `hardlinks`: Wire protocol preservation of hardlink graphs.
