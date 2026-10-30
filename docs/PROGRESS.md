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
- [ ] **Phase 1: Local Recursive File Copy & Traversal**
  - [ ] Implement `ssync-fs` path handling & directory walker.
  - [ ] Implement basic metadata reader/writer (mode, mtime, symlinks).
  - [ ] Implement `ssync-cli` argument parser matching rsync subset (`-a, -r, -l, -p, -t, -d, -v, -q, -n`).
  - [ ] Pass target upstream tests: `hands`, `dirs`, `links`, `trimslash`, `relative`.
- [ ] **Phase 2: Delta-Transfer Algorithm & Checksum Engine**
  - [ ] Implement `Checksum1` rolling checksum in `ssync-core`.
  - [ ] Implement strong hash abstraction (MD5, XXH64, SHA1).
  - [ ] Implement block search hash table and token/literal generator.
  - [ ] Integrate delta reconstruction into file updater.
  - [ ] Pass target upstream tests: `compare`, `update`, `append`, `inplace`.
- [ ] **Phase 3: Filters, Deletion & Advanced Local Synchronization**
  - [ ] Implement filter rule parser and wildmatch engine.
  - [ ] Implement deletion logic (`--delete`, `--delete-before/during/delay/after`).
  - [ ] Implement staging modes (`--delay-updates`, `--partial`, `-b/--backup`).
  - [ ] Implement hardlink preservation (`-H`).
  - [ ] Implement itemize changes (`-i`) and stats (`--stats`).
  - [ ] Pass target upstream tests: `exclude`, `delete`, `delay-updates`, `backup`, `hardlinks`, `itemize`.
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
| `hands` | Required | Baseline Passed (upstream) | Phase 1 | Canonical recursive tree transfer |
| `dirs` | Required | Baseline Passed (upstream) | Phase 1 | Non-recursive directory transfer (`-d`) |
| `links` | Required | Baseline Passed (upstream) | Phase 1 | Symlink preservation (`-l, -L, -k`) |
| `trimslash` | Required | Baseline Passed (upstream) | Phase 1 | Trailing slash path semantics |
| `relative` | Required | Baseline Passed (upstream) | Phase 1 | Relative and implied directory paths (`-R`) |
| `compare` | Required | Baseline Passed (upstream) | Phase 2 | Checksum comparison and skip rules (`-c, -I`) |
| `update` | Required | Baseline Passed (upstream) | Phase 2 | Update-only semantics (`-u`) |
| `append` | Required | Pending | Phase 2 | In-place append verification |
| `inplace` | Required | Pending | Phase 2 | In-place block updating |
| `merge` | Required | Baseline Passed (upstream) | Phase 3 | Multi-source merge into single target |
| `exclude` | Required | Pending | Phase 3 | Exclude and filter rule evaluation |
| `delete` | Required | Baseline Passed (upstream) | Phase 3 | Deletion modes |
| `hardlinks` | Required | Baseline Passed (upstream) | Phase 3 | Hardlink detection and creation (`-H`) |
| `itemize` | Required | Baseline Passed (upstream) | Phase 3 | Itemized changes output (`-i`) |
| `00-hello` | Deferred | Pending | Phase 4 | Remote server smoke test |

*(Full mapping of all 374 upstream tests is maintained in `docs/TESTSUITE_MAP.md`)*

---

## 3. Backlog

- [ ] Investigate `fallocate` / `posix_fallocate` support on Linux for preallocating destination files.
- [ ] Benchmark AVX2 / NEON vectorization for `get_checksum1` rolling updates.
- [ ] Consider adding a `--json` output flag (in addition to standard rsync output) for machine readability.
- [ ] Evaluate fuzz testing harnesses for the filter rule and wildmatch parser.

---

## 4. Notes for Next Session (Phase 1)

1. **Immediate Focus**: Implement Phase 1: Local Recursive File Copy & Traversal.
2. **Key Crates to Populate**:
   - `crates/ssync-fs`: Implement directory scanner (`walkdir` or recursive `std::fs::read_dir`) capturing path, metadata (`st_mode`, `mtime`, `size`), and symlink target.
   - `crates/ssync-cli`: Set up `clap` CLI flags matching the Phase 1 subset: `-r, -l, -p, -t, -o, -g, -a, -d, -n, -v, -q`.
   - Wire up the local copy loop: iterate scanned files, create directories, copy regular files, recreate symlinks, set metadata.
3. **Verification**: Run `scripts/run_upstream_tests.sh hands dirs links relative` and verify they pass cleanly.
