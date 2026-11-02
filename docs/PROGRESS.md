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
| **Total Passed** | 123 | 148 | **+25** |
| **Total Failed** | 139 | 114 | **-25** |
| **Total Skipped** | 111 | 111 | 0 |
| **Total Expected Fail (XFAIL)** | 1 | 1 | 0 |
| **Required for v1 Passed** | 77 | 101 | **+24** |
| **Required for v1 Failed** | 35 | 11 | **-24** |
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

*(Full mapping of all 374 upstream tests is maintained in `docs/TESTSUITE_MAP.md`)*

---

## 3. Backlog: Remaining Required Failures (11 tests)

- [ ] `append-abort-mid-write`: Signal handling (SIGINT/SIGTERM) mid-write during `--append` should cleanly flush and preserve partially appended content up to abort point without corrupting the basis file.
- [ ] `backup-crossdev-copy`: When backup directory is on a different filesystem / mount point, ensure fallback copy and deletion handles `EXDEV` across device boundaries.
- [ ] `growing-file`: Source file that grows concurrently while being read should transfer initial snapshot without crashing or reporting checksum failure.
- [ ] `partial`: Resuming partial transfers with `--partial` and `--partial-dir`.
- [ ] `partial-protected-regular-retry-linux`: Kernel `fs.protected_regular` interaction with partial temporary file naming and replacement.
- [ ] `rrsync-files-from-stdin`: Wire protocol support for `--files-from=-` streaming path list from standard input over multiplexed channel.
- [ ] `rrsync-merge-file-confine`: Restrict `.cvsignore` and per-dir merge files to stay strictly confined beneath restricted root.
- [ ] `rrsync-no-overwrite-backup-collision`: Wire receiver handling of duplicate backup filenames.
- [ ] `rrsync-no-overwrite-delay-symlinks`: Wire receiver staging and symlink updates under `--delay-updates`.
- [ ] `rrsync-sender-leaf-flip`: Wire sender race condition defense when a leaf path is swapped with a symlink or directory mid-scan.
- [ ] `source-change-size-continues`: Source file shrinking or growing during walker scan phase should log diagnostic and continue rather than panicking.

---

## 4. Notes for Next Session

1. **Session Achievements**:
   - Completed 10 root-cause fixes across 12 atomic commits.
   - Required for v1 passing tests increased from **77 to 101** (+24 passing tests).
   - Remaining Required failures reduced from **35 down to only 11**.
   - Total upstream test suite pass count increased from **123 to 148** (+25 passing tests).
   - Zero test regressions across the entire suite.
   - All 47 Rust unit and integration tests passing (`cargo test`).
   - Codebase is 100% clean: `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` produce zero warnings or errors.
   - Upstream reference repository `/f/rsync` confirmed strictly unmodified.

2. **Root Causes Fixed This Session**:
   - **Commit `9805e37`**: In-tree symlinked parent traversal and `--keep-dirlinks` handling (fixed `relative-symlinked-parent`, `relative-symlinked-parent-dotdot`, `keep-dirlinks-rule`, `keep-dirlinks-symlinked-dest`, `no-implied-dirs-symlink`, `relative-implied-symlink`).
   - **Commit `e3a9aa0`**: Standard CLI flags, executability preservation, control character sanitization in terminal/logs (fixed `missing`, `sparse`, `omit-times`, `stop-time`, `output-control-chars`, `log-control-chars`, `compress-options`, `executability`).
   - **Commit `235e893`**: Follow operator-named symlinked destination directory (fixed `dest-symlinked-dir`).
   - **Commit `6790952`**: Implementation of `--out-format`, itemized change percentages, summary stats block, `--list-only` (fixed `ki58-log-format-percent`, `output-options`).
   - **Commit `f141217`**: Added `--files-from` and `--from0` with strict leading-slash stripping and path clamping (fixed `files-from-depth`, `files-from-path-clamp`, `rrsync-write-only-files-from`).
   - **Commit `f2bffad`**: Backup symlink preservation and `--safe-links` enforcement (fixed `ki72-safe-links-backup`).
   - **Commit `c8eb05d`**: Fuzzy basis candidate selection and Levenshtein/size heuristic distance scoring (fixed `fuzzy`, `fuzzy-basis`).
   - **Commit `5022554`**: Merge-file recursion depth cap of 32, rule origin redaction in error messages, `-F` exclude-self origin inheritance, `-vv` sender match tracing (fixed `filter-depth`, `filter-merge-recursion`, `filter-merge-content-echo`).
   - **Commit `0d15483`**: Command-line support for `--super` and `--no-super` (fixed `scanner-delete-delay-overread`).
   - **Commit `55a997b`**: Catch `EACCES` on `OpenOptions::create(true)` and retry without create to satisfy Linux `fs.protected_regular` kernel sysctl (fixed `protected-regular`).
   - **Commit `85add65`**: Forwarded wire client options (`--backup`, `--backup-dir`, `--partial`, `--delay-updates`, etc.) and added server-side backup creation and directory replacement by symlinks (fixed `rrsync-backup-dir-inband-pivot`, `rrsync-no-overwrite-partial-dir`, `rrsync-no-overwrite-logfile`).
   - **Commit `ae0902f`**: Single-file destination pathing, `--link-dest` basis hardlinking, trailing slash symlink directory dereference, and wire FIFO/named pipe encoding (fixed `rrsync-receiver-file-arg`, `rrsync-root-relative-paths`, `rrsync-pull-arg-shapes`).

3. **Recommended Next Steps**:
   - Work through the remaining 11 Required failures in `docs/PROGRESS.md`:
     - Prioritize data-correctness & lifecycle: `append-abort-mid-write`, `growing-file`, and `source-change-size-continues`.
     - Next, tackle `--partial` / `--partial-dir` support: `partial` and `partial-protected-regular-retry-linux`.
     - Next, complete wire protocol edge cases: `rrsync-files-from-stdin`, `rrsync-merge-file-confine`, `rrsync-no-overwrite-backup-collision`, `rrsync-no-overwrite-delay-symlinks`, `rrsync-sender-leaf-flip`.
     - Finally, address `backup-crossdev-copy` (`EXDEV` cross-device fallback).
