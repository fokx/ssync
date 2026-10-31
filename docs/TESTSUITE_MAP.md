# TESTSUITE_MAP.md: Upstream Testsuite Architecture & Classification

This document maps every single automated test in the upstream rsync testsuite (`/f/rsync/testsuite`, at git commit `8718cd28c542985129aa6f5375d970f3b9c86bb1`, `v3.5.1-25-g8718cd28`).

---

## 1. Harness Execution & Environment

### How the Tests Locate and Invoke rsync
- The upstream harness is driven by `/f/rsync/runtests.py`.
- **Key Flags and Environment Variables**:
  - `--rsync-bin PATH`: Path to the rsync binary under test (exported as `RSYNC` into the test environment).
  - `--rsync-bin2 PATH`: Optional peer binary for two-sided testing (exported as `RSYNC_PEER`).
  - `--tooldir DIR`: Directory containing built helper binaries (`tls`, `trimslash`, `t_unsafe`, etc.) and `config.h`. Defaults to current working directory.
  - `--srcdir DIR`: Upstream source root containing `testsuite/` and `support/`.
  - `PYTHONPATH`: Prepended with `<srcdir>/testsuite` so tests can `import rsyncfns`.
- **Scratch Directory**: Each test executes in an isolated directory `<tooldir>/testtmp/<testname>/`.
- **Exit Code Convention**:
  - `0`: PASS
  - `1`: FAIL
  - `2`: ERROR
  - `77`: SKIP (reason logged to `$scratchdir/whyskipped`)
  - `78`: XFAIL

### Invoking a Single Test or the Suite
- **Single test**:
  ```bash
  python3 /f/rsync/runtests.py --srcdir=/f/rsync --tooldir=/f/ssync/target/upstream-tooldir --rsync-bin=/f/ssync/target/debug/ssync hands
  ```
- **Pattern / Glob**:
  ```bash
  python3 /f/rsync/runtests.py --srcdir=/f/rsync --tooldir=/f/ssync/target/upstream-tooldir --rsync-bin=/f/ssync/target/debug/ssync 'delete*'
  ```
- **Full testsuite**:
  ```bash
  python3 /f/rsync/runtests.py --srcdir=/f/rsync --tooldir=/f/ssync/target/upstream-tooldir --rsync-bin=/f/ssync/target/debug/ssync -j 8
  ```
- **Thin Wrapper Script**:
  We provide `scripts/run_upstream_tests.sh` in our repo to automatically build helper tools out-of-tree and set all required flags without modifying `/f/rsync`.

---

## 2. Classification Summary

Total upstream test files analyzed: **374**
- **Required for v1**: **121** tests (local tree walking, metadata preservation, delta algorithm, filters, exclude rules, deletion, atomic update modes, itemize formatting).
- **Deferred to Later Phases**: **13** tests (remote shell / server protocol compatibility over `lsh.sh` / SSH).
- **Out of Scope for v1**: **240** tests (POSIX ACLs, extended attributes, multi-tenant daemon / `rsyncd.conf`, C unit test harnesses `t_*`, raw wire packet crafting / CVE fuzzing, batch mode, root-gated device/chroot tests).

---

## 3. Complete Test Inventory and Classification

| # | Test Name | Classification | Planned Phase | Reason | Feature / Description |
|---|-----------|----------------|---------------|--------|-----------------------|
| 1 | `00-hello` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Foundational smoke test: --version / --info=help / --debug=help all work, plus a round-trip transfer of a directory whose name... |
| 2 | `acl-symlink-race` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Symlink-race (TOCTOU) LPE in the receiver's ACL-apply path. |
| 3 | `acls-default` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Test that rsync obeys POSIX default ACLs on the destination's parent directory when creating the transfer's container directory, even... |
| 4 | `acls-depth` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Coverage of -A (acls) at depth. |
| 5 | `acls-unpinnable` | Required | Phase 1 | Local sync behavior | --acls must update a destination ACL even when the destination entry cannot be pinned with O_RDONLY\ |
| 6 | `acls` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Test that rsync -A preserves POSIX ACLs across a transfer. |
| 7 | `aligned_overflow` | Out of scope | N/A | C unit test binary / internal C data structure probe | aligned_i overflow in hash_search() (--inplace > 2 GiB). Reported by Mominul Hemal |
| 8 | `alt-dest-deep` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Property-level coverage of --link-dest / --copy-dest / --compare-dest at depth and across directory boundaries. |
| 9 | `alt-dest-symlink-race` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for the basedir-confinement gap in secure_relative_open(): a parent symlink ON basedir is followed unrestrictedly... |
| 10 | `alt-dest` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Exercise rsync's --compare-dest, --copy-dest and --link-dest alternative-destination options, both locally and across the lsh.sh... |
| 11 | `append-abort-mid-write` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | An --append or --inplace transfer that is aborted while the receiver is writing out its buffer must leave the destination a clean... |
| 12 | `append-shortsum` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Regression: a short transfer checksum must not over-state the block s2length. |
| 13 | `append` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Coverage of --append and --append-verify at depth. |
| 14 | `atimes` | Required | Phase 1 | Local sync behavior | Test that rsync preserves source atimes when the binary was built with atimes support. |
| 15 | `backup-acl-xattr-cache` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Coverage: cache_tmp_acl/cache_tmp_xattr (and cache_rsync_acl/uncache_duo_acls) in backup.c make_backup_dir_tree(). |
| 16 | `backup-crossdev-copy` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage: backup.c make_backup() copy-fallback for non-regular files. |
| 17 | `backup-deep` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Property-level coverage of --backup / --suffix / --backup-dir at depth. |
| 18 | `backup-dir-relative` | Required | Phase 1 | Core local file tree copy and structure | Pin that a relative `--backup-dir` is relative to the DESTINATION dir. |
| 19 | `backup-dir-repeated-separator-delete` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Repeated backup-dir separators must not replace the backup directory. |
| 20 | `backup-dir-symlink-race` | Out of scope | N/A | Requires root privileges or OS device creation | Non-daemon (local) --backup-dir parent-component symlink-race confinement. |
| 21 | `backup-incremental` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Normal-operation test for --backup / --backup-dir, modelling the maintainer's long-standing incremental rsync backup script. |
| 22 | `backup` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Walk through --backup behaviour: * a plain backup leaves the old file at name~ alongside the new one, * --backup-dir relocates the old... |
| 23 | `bare-do-open-symlink-race` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Codex audit Findings 3b, 3c-symlink and 3c-mknod: bare do_open / do_symlink / do_mknod paths on the receiver follow parent symlinks... |
| 24 | `basis-xname-traversal` | Out of scope | N/A | C unit test binary / internal C data structure probe | Server-controlled alternate-basis xname must stay confined to the operator basedir on the client. |
| 25 | `batch-file-symlink` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | --write-batch / --read-batch must not follow planted symlinks at attacker-controlled path components. |
| 26 | `batch-mode` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Test rsync's --write-batch / --read-batch / --only-write-batch flags, both for local transfers and for daemon source/destination. |
| 27 | `batch-only-remove-source-regression` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | A batch-only run must not remove a source it has not installed. |
| 28 | `change-shrink` | Required | Phase 1 | Local sync behavior | Change-while-transferring: a source file that SHRINKS between flist-scan and send. |
| 29 | `change-vanish` | Required | Phase 1 | Local sync behavior | Change-while-transferring: a source file that is DELETED between flist-scan and send. |
| 30 | `chdir-symlink-race` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for the symlink-TOCTOU bug at the receiver's chdir(). |
| 31 | `checksum-zero-blocklen` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A malicious RECEIVER (its generator) can put a count>0 / blength=0 checksum header on the receiver->sender path; the sender's... |
| 32 | `chgrp` | Required | Phase 1 | Local sync behavior | Test that -g preserves group ownership when the user is a member of the target group. |
| 33 | `chmod-option` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Test --chmod and the daemon-side "incoming chmod = ..." setting. |
| 34 | `chmod-setid` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Regression for KI-55: --chmod=a+s must set BOTH setuid and setgid (like chmod(1)). |
| 35 | `chmod-symlink-race` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for the symlink-TOCTOU class of bug applied to chmod() on the receiver side. |
| 36 | `chmod-temp-dir` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Like chmod_test.py, but uses --temp-dir pointing at a different filesystem so rsync must rename(2) across filesystems (i.e. fall back... |
| 37 | `chmod` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Test that varied read-only and set[ug]id permissions transfer correctly both on first copy (whole-file) and on subsequent updates... |
| 38 | `chown-fake` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Verifies that rsync -a + ownership-preservation sets the destination uid/gid to match the source. |
| 39 | `chown` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Verifies that rsync -a + ownership-preservation sets the destination uid/gid to match the source. |
| 40 | `chroot-alt-dest-inner-module` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `/./` inner-module chroot daemon does not use an outside `--compare-dest` basis to suppress transfer. |
| 41 | `chroot-basis-forge-inner-module` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Finding (codex review of [16-19]): the generator-side alt-dest basis stat is confined for a /./ inner-module chroot, but the... |
| 42 | `chroot-copy-dest-inner-module` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A /./ inner-module chroot daemon must not use an out-of-inner-module file as a --copy-dest basis (here reached via inner/linkparent ->... |
| 43 | `chroot-link-dest-inner-module` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `/./` inner-module chroot daemon does not hardlink an outside `--link-dest` basis into inner module. |
| 44 | `chroot-receiver-write-inner-module` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `/./` inner-module chroot daemon receiver write does not escape inner module through symlinked parent. |
| 45 | `chroot-special-inner-module` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `/./` inner-module chroot daemon symlink creation does not escape inner module through symlinked parent. |
| 46 | `clean-fname-collapse` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for KI-50: clean_fname(name, CFN_COLLAPSE_DOT_DOT_DIRS) must collapse ".." components. |
| 47 | `clean-fname-underflow` | Required | Phase 1 | Local sync behavior | Ensure clean_fname() does not read-before-buffer when collapsing "..". |
| 48 | `compare` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Coverage of the comparison/skip options at depth: -c, -I, --size-only, --modify-window. |
| 49 | `compress-options` | Required | Phase 1 | Local sync behavior | Breadth coverage of the algorithm-selection options at depth: --compress-choice / --compress-level / --skip-compress and... |
| 50 | `compress-zlib-insert` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for issue #951. |
| 51 | `connect-prog-host-quoting` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `RSYNC_CONNECT_PROG` `%H` expansion does not allow shell injection from crafted hostnames. |
| 52 | `connect-prog-nested-singlequote-host-injection` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | RSYNC_CONNECT_PROG host quoting composes unsafely with single quotes. |
| 53 | `contimeout-rsh` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | --contimeout is documented as a "daemon connection timeout". |
| 54 | `copy-dest-source-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for codex audit Finding 3a: copy_file()'s source open in copy_altdest_file() is via do_open_nofollow(), which only... |
| 55 | `copy-dest-symlink-readleak` | Out of scope | N/A | Requires root privileges or OS device creation | --copy-dest parent-component symlink-race: out-of-tree basis read-leak. |
| 56 | `copy-xattrs-symlink-race` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Symlink-race (TOCTOU) in a confined receiver's metadata xattr write. |
| 57 | `crtimes` | Required | Phase 1 | Local sync behavior | Test that rsync preserves source create times when the binary was built with crtimes support. |
| 58 | `cvs-exclude` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | Coverage of -C / --cvs-exclude at depth. |
| 59 | `daemon-access-hostname` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon forward-DNS hostname rules against IPv4 and IPv6 peers. |
| 60 | `daemon-access-idn` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: IDN hosts allow / hosts deny matching (access.c). |
| 61 | `daemon-access-ip` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: hosts allow / hosts deny IP and CIDR matching (access.c). |
| 62 | `daemon-access` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: module path, read only, write only, list. |
| 63 | `daemon-argv-limit` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify daemon rejects excessive client argument counts over the wire (argument limit DoS protection). |
| 64 | `daemon-auth-digest-floor` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon 'auth digest' floor: refuse an auth digest weaker than a configured minimum. |
| 65 | `daemon-auth-group` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: `auth users = @group` matching and `gid = *`. |
| 66 | `daemon-auth-users-comma-only` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A leading comma in "auth users" must make splitting comma-only. |
| 67 | `daemon-auth` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: auth users, secrets file, strict modes. |
| 68 | `daemon-chroot-acl` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for GHSA-rjfm-3w2m-jf4f: a hostname-based "hosts deny" rule must still match when the daemon performs a 'daemon... |
| 69 | `daemon-chroot-munge-default` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Pin WHEN the daemon applies default symlink munging, by config regime. |
| 70 | `daemon-chroot` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: `use chroot = yes` and the `path = /outer/./inner` chroot-with-inner-subdir syntax. |
| 71 | `daemon-config-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon ancillary opens (motd / early-input / lock / --config rsyncd.conf) must not follow planted symlinks at attacker-controlled path... |
| 72 | `daemon-config` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: the &include config directive (params.c include_config / parse_directives) and a module whose path doesn't exist... |
| 73 | `daemon-copy-links-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must follow an IN-TREE symlink in its served module when the client asks to dereference it (-L copy-links / -k copy-dirlinks)... |
| 74 | `daemon-copylinks-intree` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon sender must follow an in-tree symlink for -L/--copy-links. |
| 75 | `daemon-copylinks-parent-escape` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | --copy-links may follow ".." inside the module, but never out of it. |
| 76 | `daemon-copylinks-parent-target-regression` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon --copy-links must follow a safe parent-relative symlink target. |
| 77 | `daemon-delete-msg-overflow` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A quiet daemon delete must not queue every deleted pathname. |
| 78 | `daemon-delete-stats` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon upload delete stats report deleted files. |
| 79 | `daemon-deny-dns-failopen` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon: a `hosts deny` hostname token that cannot be resolved must fail CLOSED. |
| 80 | `daemon-dot-file-force-wipe` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A write client must not use a forged dot entry to wipe a daemon module. |
| 81 | `daemon-early-exec-nameconv` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: `early exec` and `name converter` module hooks. |
| 82 | `daemon-exclude-from-outside-module` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon's own "exclude from" file may live outside the module root. |
| 83 | `daemon-exclude-namebased` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | The daemon exclude/filter matches the LOGICAL module-relative NAME, anchored at the module root -- it is a visibility/tamper filter... |
| 84 | `daemon-exec-metachar-documented-limit` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | The hook metacharacter refusal applies to operator values too, as documented. |
| 85 | `daemon-exec-metachar-refused` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A peer value carrying shell syntax must not reach a shell-executed hook. |
| 86 | `daemon-exec-rsync-env-shell-escape` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify daemon pre-xfer/post-xfer exec environment variables sanitize/prevent shell escape sequences. |
| 87 | `daemon-exec-second-shell-argument-injection` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A peer value with no EXECUTING character still selects a command. |
| 88 | `daemon-exec-singlequote-injection` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | PoC: shell-escaped %RSYNC_USER_NAME% is unsafe inside single quotes. |
| 89 | `daemon-exec` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: pre-xfer exec and post-xfer exec. |
| 90 | `daemon-files-from-subdir` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | --files-from from a module subdir must not send or remove a filter-excluded module-root file. |
| 91 | `daemon-filter-merge-bypass` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon-side --filter merge-file bypasses the module's daemon_filter_list. |
| 92 | `daemon-filter` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: exclude, incoming chmod, outgoing chmod (at depth). |
| 93 | `daemon-groupmap-wild` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for issue #829. |
| 94 | `daemon-gzip-download` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Download a file tree over a compressed connection from a test daemon. |
| 95 | `daemon-gzip-upload` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Upload a file tree over a compressed connection to a test daemon. |
| 96 | `daemon-handshake-timeout` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Exercise the absolute deadline on peer-controlled daemon handshake reads. |
| 97 | `daemon-http-proxy` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Coverage: RSYNC_PROXY HTTP CONNECT support (socket.c establish_proxy_connection() + open_socket_out()'s proxy branch + authenticate.c... |
| 98 | `daemon-include-maxconn` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: rsyncd.conf `&include`/`&merge` directives + `max connections`. |
| 99 | `daemon-leaf-type-race-fchmod` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A leaf type race must not make the receiver chmod a directory. |
| 100 | `daemon-link-dest-escape` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must confine a client's --link-dest/--compare-dest/--copy-dest basis dir to the served module (Leonid Bugaev May-2026... |
| 101 | `daemon-module-chdir-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon module-root chdir under `use chroot = no` must not follow planted symlinks at attacker-controlled path components. |
| 102 | `daemon-module-options` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: assorted rsyncd.conf module/global options whose code paths the suite never exercised. |
| 103 | `daemon-module-private-parent` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must serve a module whose path lies under a directory the SERVED (dropped-privilege) uid cannot traverse. |
| 104 | `daemon-munge` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: munge symlinks. |
| 105 | `daemon-namecvt-empty-response` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Daemon-side `name converter` empty-line response is treated as success with id=0, mapping unknown sender names to uid/gid 0. |
| 106 | `daemon-namecvt-newline-token` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A daemon with `numeric ids = no` and a `name converter` maps the user/group names a sender puts in the file list by piping each name... |
| 107 | `daemon-path-root-read` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Functional regression: a daemon module with `path = /` (use chroot = no) cannot send ANY file in 3.4.3 -- every read fails with... |
| 108 | `daemon-path-rsync-var` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression: a %RSYNC_*% reference in an ordinary daemon string param (path/uid/gid/...) must expand to the RAW value, not be... |
| 109 | `daemon-proxy-protocol` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Exercise the daemon's HAProxy PROXY-protocol parser (clientname.c read_proxy_protocol_header), the trusted-peer gate (clientserver.c... |
| 110 | `daemon-refuse-compress-threads-alias` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | ``refuse options = compress-threads`` must also refuse its ``--zt`` alias. |
| 111 | `daemon-refuse-compress` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon module configured with "refuse options = compress" must reject clients that ask for compression, and still serve the same... |
| 112 | `daemon-refuse-delete-alias` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A refuse rule naming one spelling of an option must cover the others. |
| 113 | `daemon-refuse` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: refuse options (a named option, a wildcard, and the allow-list negation form). |
| 114 | `daemon-scan-cwd-desync` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon directory scan must anchor confinement at the module root by IDENTITY, not by climbing ".." up from the cwd by the lexical... |
| 115 | `daemon-scan-dir-escape` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon sender must not let a symlink-following mode (-L / --copy-dirlinks / --copy-unsafe-links) escape the served MODULE during the... |
| 116 | `daemon-secrets-file-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon 'secrets file = PATH' must not follow planted symlinks at attacker- controlled path components. |
| 117 | `daemon-size-arg-overflow` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for an out-of-bounds write in options.c parse_size_arg(). |
| 118 | `daemon-standalone-detach` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon coverage: standalone detaching listener. |
| 119 | `daemon-stdin-local-socket` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A local stdin socket must not make a standalone daemon enter inetd mode. |
| 120 | `daemon-stdin-unix-listener` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A named Unix listener on stdin selects inetd-style daemon mode. |
| 121 | `daemon-strict-modes-matrix` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Pin the EXACT secrets-file permission contract enforced by `strict modes`. |
| 122 | `daemon-subdir-climb-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression: a daemon subdir pull in a symlink-following mode must still follow an IN-MODULE symlink whose relative target climbs with... |
| 123 | `daemon-symlink-escape-matrix` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Lock down daemon in-module-symlink resolution for a writable, non-chroot module, across every combination of: |
| 124 | `daemon-unix-socket-atfd` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify receiver `do_mknod_at` creates AF_UNIX sockets safely via held directory fd without escaping module. |
| 125 | `daemon-zstd-thread-exhaustion` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A daemon client must not control an unbounded Zstandard worker count. |
| 126 | `daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Basic daemon-mode operations against an in-process rsyncd: list modules, list a hidden module, list a single-glob match, and the... |
| 127 | `deep-path` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Exercise the DPC_MAXDEPTH (64) boundary of the held ancestor-dirfd cache. |
| 128 | `delay-symlinks-remote` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Coverage of how --delay-symlinks is passed between client and server. |
| 129 | `delay-symlinks` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Coverage of --delay-symlinks, alone and together with --delay-updates. |
| 130 | `delay-updates-deep` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Property-level coverage of --delay-updates at depth. |
| 131 | `delay-updates` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Exercise --delay-updates: pre-seed the destination's staging directory with a stale file then re-sync; the final destination must... |
| 132 | `delete-deep` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage of the --delete family at depth, plus --max-delete, --existing and --ignore-existing. |
| 133 | `delete-missing-args-files-from` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Functional regression: --delete-missing-args with --files-from aborts the transfer with "invalid file mode 00 ... |
| 134 | `delete` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Exercise `--del` dry-run output, `--delete-before`/`--delete-during`/`--delete-after`, and `--delete-excluded`. |
| 135 | `dest-symlinked-dir` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Regression: a non-daemon receiver must FOLLOW an absolute symlinked destination directory -- the common `/backup -> /mnt/disk`... |
| 136 | `devices-fake` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Test rsync's handling of device nodes (char/block/fifo) plus the fake-super variant that encodes device numbers in the... |
| 137 | `devices` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Test rsync's handling of device nodes (char/block/fifo) plus the fake-super variant that encodes device numbers in the... |
| 138 | `dir-sgid` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Check that rsync obeys the setgid bit on the destination's parent directory when creating a new directory to hold the transferred... |
| 139 | `dirs` | Required | Phase 1 | Core local file tree copy and structure | Coverage of -d / --dirs: transfer directories without recursing into them. |
| 140 | `duplicates` | Required | Phase 1 | Local sync behavior | The same source directory can be listed many times on the command line (e.g. through shell globbing). clean_flist() is supposed to... |
| 141 | `early-input-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Client --early-input=PATH must not follow planted symlinks at attacker- controlled path components. |
| 142 | `exclude-implied-trailing-backslash` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a 1-byte heap overflow in add_implied_include() (exclude.c). |
| 143 | `exclude-lsh` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Test rsync's exclude / include / filter rules, including the more obscure wildcard cases, per-directory filter files, CVS-style... |
| 144 | `exclude` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Test rsync's exclude / include / filter rules, including the more obscure wildcard cases, per-directory filter files, CVS-style... |
| 145 | `excludefrom-symlink` | Out of scope | N/A | Requires root privileges or OS device creation | --exclude-from / --include-from / --files-from / --filter=. |
| 146 | `executability` | Required | Phase 1 | Local sync behavior | Test --executability (-E): -E should propagate only the executable bits from source to destination (other permission changes ignored)... |
| 147 | `fake-super-acl-xattr` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Coverage: --fake-super -A stores/loads ACLs via the user.rsync.%aacl / %dacl xattrs (xattrs.c... |
| 148 | `fake-super-backup-fifo-regression` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Fake-super backup fallback must retain an inert FIFO placeholder. |
| 149 | `file-to-file-mkpath-dry-run` | Required | Phase 1 | Local sync behavior | Regression test for issue #880 (and the dry-run itemize regression that the first proposed fix, PR #952, would have introduced). |
| 150 | `files-from-depth` | Required | Phase 1 | Local sync behavior | Coverage of --files-from, -0/--from0, --exclude-from, --include-from at depth. |
| 151 | `files-from-leak` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `--files-from` does not leak file contents or follow untrusted symlinks outside the allowed tree. |
| 152 | `files-from-path-clamp` | Required | Phase 1 | Local sync behavior | Verify `--files-from` clamps/rejects traversal paths (`..`) so remote/daemon sender stays within root. |
| 153 | `files-from` | Deferred | Backlog | `--files-from` list reading and remote host resolution | Verify that --files-from=LIST drives rsync correctly both for a plain local sync and across the lsh.sh "remote shell" with each of the... |
| 154 | `filter-depth` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | Coverage of --exclude / --include / --filter / -F at depth. |
| 155 | `filter-leak` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify per-directory merge filter files (`--filter=dir-merge`) do not follow symlinks to read outside files. |
| 156 | `filter-merge-content-echo` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | A merge file's contents must not come back in a filter syntax error. |
| 157 | `filter-merge-recursion` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | Regression test for unbounded merge-file include recursion in the filter parser (exclude.c parse_filter_file <-> parse_filter_str). |
| 158 | `filter-merge-symlink` | Out of scope | N/A | Requires root privileges or OS device creation | Per-directory filter merge files (.cvsignore under -C, dir-merge rules) must not follow a planted symlink at an attacker-controlled... |
| 159 | `fuzzy-basis` | Required | Phase 1 | Local sync behavior | Coverage of --fuzzy basis selection scoring (util1.c fuzzy_distance). |
| 160 | `fuzzy` | Required | Phase 1 | Local sync behavior | Test --fuzzy: with a matching-content file already in the destination under a different name, rsync should use it as a basis for the... |
| 161 | `git-set-file-times-python-compat-regression` | Out of scope | N/A | C unit test binary / internal C data structure probe | The support utility must start on the suite's supported Python. |
| 162 | `growing-file` | Required | Phase 1 | Local sync behavior | Regression: a file that grows between file-list scan and data send must not abort the whole transfer. |
| 163 | `hands` | Required | Phase 1 | Core local file tree copy and structure | The canonical end-to-end transfer test: build a richly-populated source tree via hands_setup() then run a series of rsync invocations... |
| 164 | `hardlinks-deep` | Required | Phase 1 | Core local file tree copy and structure | Coverage of -H across directory boundaries. |
| 165 | `hardlinks` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Verify rsync -H detects hard links and re-creates them on the receiver. |
| 166 | `hashsearch-chain` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Regression test for issue #217: hash_search() must not blow up to O(N^2). |
| 167 | `hashsearch-collision-skip` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Regression test for hash_search() collision-flood bounds and skip boundaries. |
| 168 | `hashsearch-dup-chain` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Verify `hash_search()` handles long duplicate checksum chains properly without pathology or corruption. |
| 169 | `hashtable-overflow` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for the hashtable size*node_size integer overflow (KI-11/12). |
| 170 | `highfd-hang` | Required | Phase 1 | Local sync behavior | Regression test for issue #231: high-numbered I/O descriptors. |
| 171 | `idn` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify that rsync converts an IDN (internationalized domain name) host to its IDNA A-label (Punycode) form, and that it leaves an... |
| 172 | `inband-modname-leak` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | KI-17: start_inband_exchange leaks modname on early error returns. |
| 173 | `inplace` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Coverage of --inplace at depth. |
| 174 | `insecure-links-admin-optout` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Admin per-module opt-out: `insecure links = yes` (rsyncd.conf). |
| 175 | `install-strip` | Out of scope | N/A | C unit test binary / internal C data structure probe | Verify `make install-strip` installs rsync and invokes the configured STRIP tool on the installed binary. |
| 176 | `io-noop-flood-recursion` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for unbounded recursion in read_a_msg() (io.c). |
| 177 | `io-nosend-flood-recursion` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Sibling of io-noop-flood-recursion: the same read_a_msg() re-entry bug reached through MSG_NO_SEND instead of MSG_NOOP. |
| 178 | `io-readargs-argv-nullwrite` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for an off-by-one heap write in read_args() (io.c). |
| 179 | `itemize` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Test the output of various copy commands to ensure itemized output (-i, -ii) and double-verbose output (-vv) match the canonical... |
| 180 | `iwildmatch-fold` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for KI-53: iwildmatch() must fold case on BOTH the text and the pattern. |
| 181 | `keep-dirlinks-rule` | Required | Phase 1 | Core local file tree copy and structure | Verify `--keep-dirlinks` (-K) interaction with filter/exclude rules and directory symlinks. |
| 182 | `keep-dirlinks-symlinked-dest` | Required | Phase 1 | Core local file tree copy and structure | --keep-dirlinks (-K) into a symlinked destination directory must follow the symlink and write into the real dir -- that is the entire... |
| 183 | `ki58-log-format-percent` | Required | Phase 1 | Local sync behavior | Regression test: `%%` in --out-format outputs a literal `%`. |
| 184 | `ki62-io-error-mask` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verifies: SW-REQ-114 When MSG_IO_ERROR arrives, io.c OR's the wire value into the local io_error. |
| 185 | `ki72-safe-links-backup` | Required | Phase 1 | Core local file tree copy and structure | KI-72: --safe-links must be honored in the backup symlink hard-link path. |
| 186 | `ki73-cvs-clear-list` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | KI-73 regression: a `.cvsignore` containing only `!` (the clear-list token) must not abort rsync in -C (cvs-exclude) mode. |
| 187 | `link-dest-module-escape` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Security guard for the #915 re-anchor: a daemon receiver must NOT honour an alt-basis dir whose `..` climbs OUT of the module. |
| 188 | `link-dest-pathroot` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Functional regression: a relative --link-dest=../sibling against a daemon module with `path = /` (the intersection of #897 and #915). |
| 189 | `link-dest-relative-basis` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Functional regression: a RELATIVE alt-basis dir (--link-dest / --copy-dest / --compare-dest = ../sibling) is silently ignored by a... |
| 190 | `link-dest-symlink-enotsup` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | --link-dest must fall back to a copy when the destination refuses to hard-link a symlink, whatever the errno, and must report it only... |
| 191 | `links` | Required | Phase 1 | Core local file tree copy and structure | Coverage of symlink-handling options -l / -L / -k at depth. |
| 192 | `log-control-chars` | Required | Phase 1 | Local sync behavior | Regression for KI-51/52: the log file must escape control characters in a (possibly attacker-controlled) filename, so an admin who... |
| 193 | `log-file-symlink` | Out of scope | N/A | Requires root privileges or OS device creation | --log-file=PATH must not follow planted symlinks at attacker-controlled path components. |
| 194 | `longdir` | Required | Phase 1 | Local sync behavior | Regression test for a 2.0.11 bug: rsync used to mishandle paths nested inside a stupidly-long directory name. |
| 195 | `macos-setgid-ordinary-mode-regression` | Required | Phase 1 | Local sync behavior | A failed setgid request must not discard the ordinary directory mode. |
| 196 | `malicious-dot-dir-delete-scope` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A malicious pull server must not enlarge --delete to the transfer root. |
| 197 | `malicious-dot-file-delete-scope` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A malicious sender must not reinterpret the destination root as a file. |
| 198 | `malicious-sender-delete-scope` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Malicious daemon-sender expands --delete scope by flipping an implied parent into a content-dir. |
| 199 | `malicious-server-partial-basis-symlink-overwrite` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A malicious pull server must not redirect an in-place partial update. |
| 200 | `match-append-empty-nullmap` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a NULL-pointer deref in match_sums() (match.c): in daemon-as-sender mode, send_files() leaves mbuf == NULL for an... |
| 201 | `match-want-i-nolen` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a NULL-pointer deref in hash_search() (match.c): the check_want_i adjacent-match optimisation substitutes i =... |
| 202 | `max-alloc-zero` | Deferred | Phase 4 | Remote shell / server protocol compatibility | ``--max-alloc=0`` means "the largest limit this build supports". |
| 203 | `merge` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | Verify that rsync merges files from multiple source directories into a single destination, with later sources NOT clobbering earlier... |
| 204 | `metadata-depth` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage of -p (perms), -t (times) and --chmod at depth. |
| 205 | `misc-coverage` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Coverage: cheap one-off paths no other test reaches. |
| 206 | `missing` | Required | Phase 1 | Local sync behavior | Verify `missing_below` handling on dry-run with `--ignore-non-existing` and missing parent directories. |
| 207 | `mkpath` | Required | Phase 1 | Local sync behavior | Test the rsync --mkpath option: it should create any missing intermediate destination directories rather than erroring out. |
| 208 | `msg-io-timeout-overflow` | Out of scope | N/A | C unit test binary / internal C data structure probe | A near-INT_MAX I/O timeout must not overflow set_io_timeout() into a tight CPU loop. |
| 209 | `msg-io-timeout-zero` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A malicious server must not be able to disable the client's --timeout. |
| 210 | `nested-socket-specials` | Required | Phase 1 | Local sync behavior | Regression: `rsync -a` (implies --specials via -D) of a tree with a NESTED unix socket must not fail the whole transfer on platforms... |
| 211 | `no-implied-dirs-symlink` | Required | Phase 1 | Core local file tree copy and structure | Pin the receiver-side symlink behaviour of -R `--no-implied-dirs`. |
| 212 | `nondaemon-symlink-race` | Out of scope | N/A | Requires root privileges or OS device creation | Non-daemon (local) transfer: parent-component symlink-race confinement. |
| 213 | `nonroot-restrictive-perms` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Coverage: the generator/delete chmod-around-restrictive-perms paths. |
| 214 | `omit-times` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage of -O (--omit-dir-times) and -J (--omit-link-times) at depth. |
| 215 | `open-noatime` | Required | Phase 1 | Local sync behavior | Test that rsync --open-noatime keeps the source atime intact across the transfer. |
| 216 | `operator-path-backup-dir-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify daemon mode ignores/refuses client `--insecure-links` for `--backup-dir` symlink traversal. |
| 217 | `operator-path-backup-dir-exclude-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must not let a peer-supplied --backup-dir reach the module's EXCLUDED subtree via a symlink. |
| 218 | `operator-path-backup-dir` | Out of scope | N/A | Requires root privileges or OS device creation | --backup-dir symlink policy matrix. |
| 219 | `operator-path-backup-rmdir` | Out of scope | N/A | C unit test binary / internal C data structure probe | Non-daemon (local) --backup-dir parent-component symlink-race confinement, for the case where a backup replaces a pre-existing backup... |
| 220 | `operator-path-backup-symlink` | Out of scope | N/A | C unit test binary / internal C data structure probe | Non-daemon (local) --backup-dir parent-component symlink-race confinement, for the case where the backed-up item is a symlink... |
| 221 | `operator-path-compare-dest` | Out of scope | N/A | Requires root privileges or OS device creation | --compare-dest symlink policy matrix. |
| 222 | `operator-path-copy-dest` | Out of scope | N/A | Requires root privileges or OS device creation | --copy-dest symlink policy matrix. |
| 223 | `operator-path-dir-daemon-inmodule` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression guard for the module-confinement / exclude-aware operator-path resolver: a LEGITIMATE absolute --partial-dir that lives... |
| 224 | `operator-path-dir-daemon-leaf` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify leaf-level module exclude (`exclude = /public/f0`) cannot be bypassed via peer `--partial-dir`/`--backup-dir`. |
| 225 | `operator-path-dir-daemon-mkdir` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A DIRECTORY-only module exclude (exclude = /public/pdir/) must not be bypassable by creating that dir through a peer --partial-dir... |
| 226 | `operator-path-dir-daemon-outside` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must not let a peer-supplied --partial-dir/--backup-dir symlink resolve OUTSIDE the served module at all (not just into an... |
| 227 | `operator-path-files-from` | Out of scope | N/A | Requires root privileges or OS device creation | --files-from symlink policy matrix (also covers the shared open site used by --include-from / --exclude-from / --filter merge in... |
| 228 | `operator-path-inplace-backup-dir` | Out of scope | N/A | Requires root privileges or OS device creation | --inplace --backup-dir symlink policy matrix. |
| 229 | `operator-path-insecure-links-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must NEVER honor a peer-forwarded --insecure-links: a client cannot be allowed to switch off the daemon's symlink confinement... |
| 230 | `operator-path-insecure-links-refused` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must DROP the connection if a client passes --insecure-links over the protocol. |
| 231 | `operator-path-link-dest` | Out of scope | N/A | Requires root privileges or OS device creation | --link-dest symlink policy matrix. |
| 232 | `operator-path-log-file` | Out of scope | N/A | Requires root privileges or OS device creation | --log-file symlink policy matrix. |
| 233 | `operator-path-partial-dir-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify daemon mode confines `--partial-dir` against symlink traversal even if `--insecure-links` is sent. |
| 234 | `operator-path-partial-dir-exclude-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | A daemon must not let a peer-supplied --partial-dir reach the module's EXCLUDED subtree via a symlink. |
| 235 | `operator-path-partial-dir` | Out of scope | N/A | Requires root privileges or OS device creation | --partial-dir symlink policy (absolute, parent-component), create and reuse. |
| 236 | `operator-path-temp-dir` | Out of scope | N/A | Requires root privileges or OS device creation | --temp-dir symlink policy matrix. |
| 237 | `operator-path-traversal-backup-dir-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon --backup-dir ".." traversal into the module's excluded subtree, on a `path = /` module. |
| 238 | `operator-path-traversal-dest-dir-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon destination ".." traversal into the module's excluded subtree, on a `path = /` module. |
| 239 | `operator-path-traversal-partial-dir-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon --partial-dir ".." traversal into the module's excluded subtree, on a `path = /` module. |
| 240 | `operator-path-write-batch` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | --write-batch symlink policy matrix (batch.c open site, shared with --read-batch / --only-write-batch). |
| 241 | `output-control-chars` | Required | Phase 1 | Local sync behavior | Verify rsync escapes control characters in filenames in normal and verbose output unless `-8` (`--8-bit-output`) is used. |
| 242 | `output-options` | Required | Phase 1 | Local sync behavior | Breadth coverage of the output / reporting options. |
| 243 | `ownership-depth` | Out of scope | N/A | Requires root privileges or OS device creation | Coverage of --groupmap / --chown / --usermap / -o at depth. |
| 244 | `partial-dir-abs-delta` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Regression guard: a --no-whole-file delta whose basis is reached via an ABSOLUTE --partial-dir must reconstruct correctly, not... |
| 245 | `partial-protected-regular-retry-linux` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | PoC: EACCES recovery must retain partial-dir ownership policy (Linux). |
| 246 | `partial-protected-regular-retry-policy` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | PoC: EACCES recovery must retain partial-dir ownership policy. |
| 247 | `partial_nowrite` | Out of scope | N/A | Requires root privileges or OS device creation | Test that --partial and --delay-updates work as expected when then permissions of the destination file prevent writing to it. |
| 248 | `partial` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage of --partial / --partial-dir at depth and across directory boundaries. |
| 249 | `password-file-first-line` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Only the first line of --password-file is used, for files and stdin. |
| 250 | `password-file-symlink` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | --password-file=PATH must not follow planted symlinks at attacker- controlled path components. |
| 251 | `peer-legacy-implied-delete-scope` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Protocol 29 must not let a sender enlarge scoped --delete. |
| 252 | `preallocate` | Required | Phase 1 | Local sync behavior | Coverage of the file-allocation syscalls in syscall.c at depth: do_fallocate (--preallocate) and do_punch_hole (sparse writes). |
| 253 | `protected-regular` | Required | Phase 1 | Local sync behavior | Modern Linux kernels can set fs.protected_regular = {1,2}, which blocks O_CREAT\ |
| 254 | `proto-cleared-dirflist` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a receiver NULL-deref on a sub-flist whose dir index targets a cleared dir_flist entry (flist.c recv_file_list). |
| 255 | `proto-cleared-ndx` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a receiver NULL-deref on a transfer-phase index that targets a cleared file-list entry (receiver.c recv_files). |
| 256 | `proto-hlink-flag-oob` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for the heap out-of-bounds WRITE in recv_file_entry() (flist.c): a malicious sender that sets XMIT_HLINKED on a file... |
| 257 | `proto-hlink-gnum` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a peer-controlled assert() abort in match_gnums() (hlink.c). |
| 258 | `proto-msg-info-assert` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a remotely-reachable abort in rwrite() (log.c): a daemon receiver child (am_server=1, am_generator=0... |
| 259 | `proto-parent-ndx-empty-dirflist` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a wild-pointer read via the first inc_recurse flist's parent_ndx when dir_flist is empty (flist.c recv_file_list). |
| 260 | `proto-sender-selftest` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Self-test / bitrot guard for the pure-Python rsync sender (rsync_proto.py). |
| 261 | `proto-subflist-freed` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Regression test for a stale-dir_flist use-after-free on a sub-flist marker sent after every flist has been freed (flist.c... |
| 262 | `proxy-connect-request-too-long` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `RSYNC_PROXY` HTTP CONNECT request rejects overly long host/port headers before buffer overflow. |
| 263 | `proxy-host-crlf` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test: with RSYNC_PROXY set, a daemon host containing CR/LF must be rejected before the HTTP CONNECT request is formatted... |
| 264 | `proxy-protocol-trusted-peer` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify daemon `proxy protocol = true` parses HAProxy PROXY v1/v2 headers only from trusted peers. |
| 265 | `proxy-response-header-too-long` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify `RSYNC_PROXY` client rejects oversized HTTP proxy response lines cleanly. |
| 266 | `proxy-response-line-too-long` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for the off-by-one stack OOB write in establish_proxy_connection() when a malicious or man-in-the-middle HTTP proxy... |
| 267 | `prune-empty-dirs` | Required | Phase 1 | Core local file tree copy and structure | Coverage of -m / --prune-empty-dirs at depth. |
| 268 | `pseudo-paths-daemon` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Verify handling of pseudo-paths and tricky relative/dot path components in daemon transfers. |
| 269 | `pseudo-paths` | Out of scope | N/A | Requires root privileges or OS device creation | Verify handling of pseudo-paths and tricky relative/dot path components in local transfers. |
| 270 | `read-batch-pipe` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | --read-batch process substitution /dev/fd/ pipe must not crash with strict file-type checks. |
| 271 | `readonly-partial-abort-mode-regression` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | An aborted in-place update must not relax an existing file's mode. |
| 272 | `recv-discard-nullderef` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for a receiver NULL-deref on the delta DISCARD path. |
| 273 | `recv-generator-acl-leak` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | KI-23: recv_generator leaks real_sx ACL data on the directory path. |
| 274 | `relative-content` | Required | Phase 1 | Core local file tree copy and structure | rsync -aR (--relative) must transfer file CONTENT, not just the directory tree. |
| 275 | `relative-implied-symlink` | Required | Phase 1 | Core local file tree copy and structure | Verify `--relative` (`-R`) handling of implied parent directories when a parent path is a symlink. |
| 276 | `relative-implied` | Required | Phase 1 | Core local file tree copy and structure | Coverage of -R implied directories and --no-implied-dirs at depth. |
| 277 | `relative-mkpath-dir-symlink` | Required | Phase 1 | Core local file tree copy and structure | --relative directory-entry creation escapes the tree via make_path(). |
| 278 | `relative-mkpath-symlink` | Required | Phase 1 | Core local file tree copy and structure | --relative implied-parent creation escapes the tree via make_path(). |
| 279 | `relative-source-ancestor` | Out of scope | N/A | Requires root privileges or OS device creation | Explicit source directory symlinks remain transfer roots. |
| 280 | `relative-symlinked-parent-dotdot` | Required | Phase 1 | Core local file tree copy and structure | rsync -aR through an in-tree symlinked parent whose target climbs with ".." but stays BENEATH the transfer anchor must transfer the... |
| 281 | `relative-symlinked-parent` | Required | Phase 1 | Core local file tree copy and structure | rsync -aR of a source file whose path goes through an IN-TREE symlinked parent directory must transfer the file -- the symlink is part... |
| 282 | `relative` | Required | Phase 1 | Core local file tree copy and structure | Exercise rsync --relative (-R) behaviour: paths anchored at a "./" cut point should reproduce that subtree at the destination. |
| 283 | `remote-shell-newline-escaping` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Finding [27]: rsync's remote-shell argument quoting (SHELL_CHARS in options.c) omitted the newline/CR command separators, so a newline... |
| 284 | `rename-fullpath-symlink-race` | Required | Phase 1 | Local sync behavior | Receiver write-escape via a symlink race on finish_transfer()'s tmp->final rename when an absolute --temp-dir (or --partial-dir) is in... |
| 285 | `rename-mixed-parent-escape-poc` | Out of scope | N/A | C unit test binary / internal C data structure probe | PoC for the 3.4.3 do_rename_at() mixed-parent fallback. |
| 286 | `rename-mixed-parent-symlink-race` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for do_rename_at() mixed top-level/slashed paths. |
| 287 | `rename-mixed-parent-transfer` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | End-to-end (real daemon transfer) companion to the unit-level rename-mixed-parent-symlink-race / rename-mixed-parent-escape-poc tests. |
| 288 | `reverse-daemon-delta` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Reverse-direction version-mixing smoke test: OLD client <-> CURRENT daemon. |
| 289 | `rrsync-alt-dest-inband-pivot` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | An in-band symlink at an alt-dest name must not read outside the restricted dir. |
| 290 | `rrsync-archive-mode` | Required | Phase 1 | Local sync behavior | A subdir-restricted rrsync must accept a normal `rsync -a` transfer. |
| 291 | `rrsync-backup-dir-inband-pivot` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | rrsync must not let one transfer create and consume an option-path symlink. |
| 292 | `rrsync-copy-unsafe-links-denied` | Required | Phase 1 | Core local file tree copy and structure | Finding [8] (CWE-59): support/rrsync leaves --copy-unsafe-links enabled in a restricted (non-"/") directory. |
| 293 | `rrsync-debug-denied` | Required | Phase 1 | Local sync behavior | rrsync must not let a peer turn on the server's --debug output. |
| 294 | `rrsync-files-from-stdin` | Required | Phase 1 | Local sync behavior | A pull with a local --files-from must work through rrsync. |
| 295 | `rrsync-logfile-symlink` | Out of scope | N/A | C unit test binary / internal C data structure probe | Verify `support/rrsync` refuses to follow a symlinked `rrsync.log` file in the restricted directory. |
| 296 | `rrsync-merge-file-confine` | Required | Phase 3 | Filter, include/exclude, and wildmatch rules | An rrsync restricted dir must bound the merge files the peer can name. |
| 297 | `rrsync-no-overwrite-backup-collision` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | rrsync -no-overwrite must not let backup publication overwrite a protected file. |
| 298 | `rrsync-no-overwrite-delay-symlinks` | Required | Phase 1 | Core local file tree copy and structure | rrsync accepts --delay-symlinks, and -no-overwrite refuses it. |
| 299 | `rrsync-no-overwrite-delay-updates` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | rrsync -no-overwrite must protect implicit delay-update partial files. |
| 300 | `rrsync-no-overwrite-logfile` | Required | Phase 1 | Local sync behavior | rrsync -no-overwrite must not let the peer append to an existing file. |
| 301 | `rrsync-no-overwrite-partial-dir` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | rrsync -no-overwrite must protect existing partial-dir files too. |
| 302 | `rrsync-pull-arg-shapes` | Required | Phase 1 | Local sync behavior | Every pull argument shape through rrsync must deliver what it names. |
| 303 | `rrsync-pull-delivers-content` | Required | Phase 1 | Local sync behavior | A pull through rrsync must deliver the file's CONTENT. |
| 304 | `rrsync-receiver-file-arg` | Required | Phase 1 | Local sync behavior | Receiver file arguments must remain beneath a pinned parent. |
| 305 | `rrsync-root-relative-paths` | Required | Phase 1 | Core local file tree copy and structure | Leading slashes through rrsync must remain relative to the restricted root. |
| 306 | `rrsync-sender-leaf-flip` | Required | Phase 1 | Local sync behavior | A raced LAST component must not leak outside content through an rrsync pull. |
| 307 | `rrsync-sender-parent-pin` | Out of scope | N/A | C unit test binary / internal C data structure probe | A sender leaf rrsync never opens must still resolve beneath a pinned parent. |
| 308 | `rrsync-specials-denied` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Finding [26]: support/rrsync leaves -D / --specials enabled in a restricted (non-"/") directory, so a restricted SSH user can have the... |
| 309 | `rrsync-symlink` | Required | Phase 1 | Local sync behavior | rrsync (the SSH-restricted-rsync wrapper) must close its realpath-vs-exec TOCTOU window so an attacker cannot flip a path component... |
| 310 | `rrsync-userns-procfs` | Out of scope | N/A | Requires root privileges or OS device creation | A confined fd pin must remain usable inside a Linux user namespace. |
| 311 | `rrsync-write-only-files-from` | Required | Phase 1 | Local sync behavior | A forced rrsync -wo endpoint must not return a local --files-from file. |
| 312 | `rsync-ssl-hostname-validation` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Finding [20] (CWE-88 argument injection): rsync-ssl expands an untrusted SSL hostspec into the openssl/gnutls command line unquoted... |
| 313 | `rsync-ssl-openssl-hostname-check` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Companion to rsync-ssl-stunnel-hostname-check for the openssl backend. |
| 314 | `rsync-ssl-stunnel-ca-required` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Finding [12] (CWE-295 improper certificate validation): in stunnel mode rsync-ssl builds the stunnel client config with NO CA / verify... |
| 315 | `rsync-ssl-stunnel-hostname-check` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Finding (codex scan, CWE-295 improper certificate validation): in stunnel mode rsync-ssl emitted "verifyChain = yes" + "CAfile" but NO... |
| 316 | `rsync-ssl-type-option` | Required | Phase 1 | Local sync behavior | rsync-ssl only recognized --type=SSL_TYPE as the FIRST argument, so "rsync-ssl --dry-run --type=stunnel host::mod" passed the option... |
| 317 | `safe-arg-leak` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for KI-54: safe_arg() in filename mode (opt==NULL) must not leak an uninitialized heap byte. |
| 318 | `safe-links-absolute-intree` | Required | Phase 1 | Core local file tree copy and structure | Absolute symlink that *resolves inside* the copied tree, under --safe-links. |
| 319 | `safe-links-unsafe-def` | Required | Phase 1 | Core local file tree copy and structure | Pin what `--safe-links` / `--copy-unsafe-links` call an "unsafe" symlink, and the "top of the transfer" cut-off. |
| 320 | `safe-links` | Required | Phase 1 | Core local file tree copy and structure | --safe-links must drop symlinks whose target escapes the transfer root. |
| 321 | `scanner-argv-bounds` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Regression tests for the run5 option/argv bound fixes: a hostile or wrapper-built command line must not overflow a fixed buffer... |
| 322 | `scanner-batch-flag-mismatch` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Regression test for run5 0016: a --read-batch whose recorded stream-flags enable a metadata class (xattrs/acls/uid/gid) that the... |
| 323 | `scanner-daemon-log-checksum` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for run5 0025: a daemon-as-sender with -c and a module `log format` containing %C read F_SUM(file) in log_formatted()... |
| 324 | `scanner-delete-delay-overread` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Regression test for run5 0017: read_delay_line() mis-sized a '!'-prefixed (DEL_NO_UID_WRITE) delete-delay entry by one byte... |
| 325 | `search-only-destination` | Out of scope | N/A | Requires root privileges or OS device creation | The receiver must traverse a searchable but unreadable destination parent. |
| 326 | `search-only-held-dirfd` | Out of scope | N/A | Requires root privileges or OS device creation | Known-name operations must not require permission to list parent dirs. |
| 327 | `secure-relpath-validation` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for codex audit Finding 5: secure_relative_open()'s front-door input check rejects "../foo" and "foo/../bar" but... |
| 328 | `sender-flist-symlink-leak` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Regression test for codex re-check finding: the sender-side file-list generator could still follow an attacker-planted symlink out of... |
| 329 | `sender-readlink-atfd` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | sender-readlink-atfd: symlink target reads did not leak through a raced parent |
| 330 | `sender-remove-source-relative-anchor` | Required | Phase 1 | Core local file tree copy and structure | Absolute --relative source cleanup must use the source's absolute parent. |
| 331 | `sender-remove-source-root-anchor` | Out of scope | N/A | Requires root privileges or OS device creation | An absolute --relative cleanup must not be re-anchored at the sender's CWD. |
| 332 | `sender-remove-source-secure` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | sender-remove-source-secure: remove-source cleanup did not unlink outside the module |
| 333 | `sender-scan-dir-escape` | Required | Phase 1 | Local sync behavior | Source-tree directory-ENUMERATION escape via a parent-component symlink race. |
| 334 | `simd-checksum` | Out of scope | N/A | C unit test binary / internal C data structure probe | Run the simdtest helper, which compares the SIMD checksum implementations against the C reference. |
| 335 | `size-filter` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Coverage of --max-size / --min-size at depth. |
| 336 | `skiplist-spec` | Required | Phase 1 | Local sync behavior | Unit test for `runtests.py` `expand_skip_spec()` (`--expect-skipped` / `RSYNC_EXPECT_SKIPPED` parser). |
| 337 | `source-change-size-continues` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Regression guard for a source file changing size during send_files(). |
| 338 | `sparse` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage of -S (--sparse) at depth. |
| 339 | `ssh-basic` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Basic two-step "remote shell" transfer via lsh.sh (or real ssh if rsync_enable_ssh_tests=yes is set in shconfig). |
| 340 | `stop-time` | Required | Phase 1 | Local sync behavior | Coverage of --stop-at (options.c parse_time) and --stop-after. |
| 341 | `strict-basis` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Ensure forged partial-dir basis tokens cannot enable in-place writes. |
| 342 | `symlink-dest-backupdir` | Out of scope | N/A | Requires root privileges or OS device creation | Destination config-path (--backup-dir) symlink confinement -- DETERMINISTIC. |
| 343 | `symlink-dirlink-basis` | Deferred | Phase 4 | Remote shell / server protocol compatibility | Regression test for https://github.com/RsyncProject/rsync/issues/715: updating a file through a directory symlink with -K... |
| 344 | `symlink-exclude-chdir-alias` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon module-exclude under-block via an ALLOWED symlinked chdir whose logical path then mislabels descendants. |
| 345 | `symlink-exclude-component` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon module-exclude bypass via an in-DESTINATION symlink that appears as a PATH COMPONENT of a transferred file (not as the... |
| 346 | `symlink-exclude-deep` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon module-exclude bypass via a DEEPER in-tree directory symlink (not the destination argument). |
| 347 | `symlink-exclude-leaf` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon module-exclude bypass on an excluded LEAF (file) reached through an in-tree directory symlink. |
| 348 | `symlink-exclude-meta` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | Daemon module-exclude bypass: metadata write on a PRE-EXISTING excluded leaf reached through an in-tree directory symlink. |
| 349 | `symlink-exclude-xattr` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | The daemon exclude/filter is name-based, not a symlink boundary: an xattr (-X) write on a pre-existing excluded leaf reached through... |
| 350 | `symlink-exclude` | Out of scope | N/A | Daemon / rsyncd.conf / multi-tenant server out of scope for v1 | The daemon exclude/filter is a NAME-based listing/tamper filter, not a physical-path security boundary: a peer that pushes through an... |
| 351 | `symlink-ignore` | Required | Phase 1 | Local sync behavior | Default behaviour: without -l, -L or -a, rsync should NOT copy any symlinks. |
| 352 | `symlink-mknod-fakesuper-symlink-race` | Out of scope | N/A | C unit test binary / internal C data structure probe | Regression test for the fake-super branches of do_symlink_at() and do_mknod_at(). |
| 353 | `symlink-race-dest` | Out of scope | N/A | Requires root privileges or OS device creation | Destination chdir symlink defense (cross-uid) -- ABSOLUTE destination. |
| 354 | `symlink-race-relative-dest` | Out of scope | N/A | Requires root privileges or OS device creation | Relative-destination chdir symlink defense (cross-uid). |
| 355 | `symlink-race-source` | Required | Phase 1 | Local sync behavior | Source-tree parent-component symlink race (TOCTOU) -- scenarios 1 & 3. |
| 356 | `symlink-unix-perms` | Required | Phase 1 | Local sync behavior | Test setting Unix access permissions on symlinks (when supported by OS) and transferring them with rsync. |
| 357 | `temp-dir-symlink-injection` | Out of scope | N/A | Requires root privileges or OS device creation | Absolute --temp-dir parent-component symlink-race: content injection. |
| 358 | `temp-dir` | Required | Phase 3 | Advanced local sync features (deletion, backup, hardlinks, itemize) | Coverage of --temp-dir (-T) at depth and across directory boundaries. |
| 359 | `trimslash` | Out of scope | N/A | C unit test binary / internal C data structure probe | Test the tiny trimslash helper which strips trailing slashes from paths. |
| 360 | `uidlist-id0-name-leak` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | KI-25: send_one_list leaks the id-0 (root) name string. |
| 361 | `unsafe-byname` | Out of scope | N/A | C unit test binary / internal C data structure probe | Call directly into the t_unsafe helper (which wraps unsafe_symlink()) and verify its judgement on a battery of crafted target/curdir... |
| 362 | `unsafe-links` | Required | Phase 1 | Core local file tree copy and structure | Verifies the three relevant policies for "unsafe" (escape-the-tree) symlinks: * default -a copies them as symlinks (no special... |
| 363 | `update` | Required | Phase 2 | Delta-transfer algorithm, checksums, and update heuristics | Coverage of -u (--update) and --force at depth. |
| 364 | `variety-symlink-traversal` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Companion to variety_test.py for the cases the user specifically called out: the rsync CLIENT is started with a path that TRAVERSES a... |
| 365 | `variety` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Differential regression test: introducing the current rsync on EITHER side of a transfer must not change the destination tree that an... |
| 366 | `wildmatch` | Out of scope | N/A | C unit test binary / internal C data structure probe | Exercise the wildmatch() function (with no options) and wildmatch_join() (via -x and/or -e) by running the wildtest helper across a... |
| 367 | `write-batch-filter-injection` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | Regression test: the generated --write-batch replay script writes filter rules into a `<<'#E#'` here-doc, one rule per line. |
| 368 | `write-batch-quoting` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | Regression test: the generated --write-batch replay script must single-quote every argument -- including an arg that starts with '-'... |
| 369 | `write-touched-blocks` | Out of scope | N/A | Batch mode (--write-batch/--read-batch) out of scope for v1 | Verify protocol 33 `--inplace -I --no-whole-file` only writes touched blocks and reports accurate literal/matched stats. |
| 370 | `xattr-wire-cap` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | A malicious DAEMON acting as the SENDER can put an oversized per-value xattr datum_len in the file list it streams to a pulling client. |
| 371 | `xattrs-depth` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Coverage of -X (xattrs) at depth. |
| 372 | `xattrs-hlink` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Test that rsync -X preserves extended attributes through a transfer, plus the --link-dest / --copy-dest / --fake-super interactions. |
| 373 | `xattrs` | Out of scope | N/A | ACL / extended attribute support out of scope for v1 | Test that rsync -X preserves extended attributes through a transfer, plus the --link-dest / --copy-dest / --fake-super interactions. |
| 374 | `xrsync` | Out of scope | N/A | Raw wire packet crafting / CVE fuzzing test against upstream C rsyncd | Tests for xrsync.py -- the small runnable rsync client built on rsync_proto. |
