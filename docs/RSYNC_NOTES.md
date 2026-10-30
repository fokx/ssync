# RSYNC_NOTES.md: Technical Architecture & Specification Notes

This document contains deep architectural notes on rsync's design, protocols, wire formats, algorithms, and invariants, synthesized directly from the upstream reference codebase (`/f/rsync` at commit `8718cd28c542985129aa6f5375d970f3b9c86bb1`, `v3.5.1-25-g8718cd28`).

---

## 1. Protocol Versions and Negotiation

### Version Boundaries & History
- Upstream Constants (`rsync.h`):
  - `PROTOCOL_VERSION`: Current default wire protocol is `33` (as of rsync 3.5.x git HEAD).
  - `SUBPROTOCOL_VERSION`: `0` for official releases.
  - `MIN_PROTOCOL_VERSION`: `20` (minimum supported version; older versions are rejected).
  - `OLD_PROTOCOL_VERSION`: `25` (legacy protocol cutoff).
  - `MAX_PROTOCOL_VERSION`: `40` (detects non-rsync handshakes).
- Evolution across protocols:
  - **Protocol 28**: Added extended file-list transmission flags (`XMIT_EXTENDED_FLAGS`), major/minor device separation (`XMIT_SAME_RDEV_MAJOR`), hardlink flag (`XMIT_HLINKED`).
  - **Protocol 29**: Added standard compatibility negotiation flags, 64-bit dev/inode support.
  - **Protocol 30**: Major redesign.
    - Switched from full file-list pre-transfer to **incremental recursion** (`--inc-recursive`, `inc_recurse`).
    - Introduced **variable-length integer encoding** (`varint`, `varlong`) in place of fixed 32-bit / 64-bit words.
    - User/group names transmitted directly in the file list (`XMIT_USER_NAME_FOLLOWS`, `XMIT_GROUP_NAME_FOLLOWS`) instead of global uid/gid translation tables.
    - Introduced `--safe-links` and sender argument protection (`-s` / `--secluded-args`).
  - **Protocol 31**:
    - Nanosecond modification time support (`XMIT_MOD_NSEC`).
    - Special file list error termination flag (`XMIT_IO_ERROR_ENDLIST`).
    - Synchronized error exit message (`MSG_ERROR_EXIT`).
    - Negotiated checksum and compression algorithms (`--checksum-choice`, `--compress-choice`).
  - **Protocol 32 / 33**:
    - Added modern hashes (XXH64, XXH3-64, XXH3-128) and modern compressors (zstd, lz4).
    - Hardening against TOCTOU symlink races and directory traversal vulnerabilities.

### Process Topology & Role Negotiation
Rsync operates across three primary processes, configured depending on the transport:
1. **Local Copy Topology** (`main.c` `do_direct_copy`, `pipe.c`):
   - The CLI process forks into **Sender** and **Receiver**.
   - Receiver in turn forks into the **Generator** (or vice versa).
   - The Generator inspects destination basis files and sends block checksums to the Sender over an IPC pipe.
   - The Sender compares source files against the checksums, emitting literal bytes and block match tokens over another pipe to the Receiver.
   - The Receiver reconstructs files, verifies digests, and sets metadata.
2. **Remote Shell Topology** (`main.c` `do_cmd`):
   - Client invokes SSH or remote shell: `ssh host rsync --server [--sender] [options] . <target>`.
   - The client handles one role (Sender or Receiver + Generator), while the remote server handles the counterpart over `stdin`/`stdout`.
3. **Daemon Mode Topology** (`clientserver.c`):
   - Client connects to `rsync://host/module` (or over `RSYNC_CONNECT_PROG`).
   - Daemon outputs greeting: `@RSYNCD: <version>\n`.
   - Client replies: `@RSYNCD: <version>\n`.
   - Authentication exchange (if configured: `@RSYNCD: AUTHREQD <challenge>` -> `@RSYNCD: <user> <digest>`).
   - Client sends module name and command arguments terminated by an empty line.

### Compatibility Flags & Wire Negotiation
- Upstream Files: `compat.c` (`compat_flags`, `setup_protocol`, `negotiate_the_strings`), `options.c` (`maybe_add_e_option`).
- Modern rsync exchanges capability flags over `-e.` or during initial handshake:
  - `CF_INC_RECURSE` (`1<<0`): Client and server support incremental recursion.
  - `CF_SYMLINK_TIMES` (`1<<1`): Symlink mtime preservation supported (`lutimes` / `utimensat`).
  - `CF_SYMLINK_ICONV` (`1<<2`): Symlink character translation.
  - `CF_SAFE_FLIST` (`1<<3`): Safe flist I/O error termination.
  - `CF_AVOID_XATTR_OPTIM` (`1<<4`): Disables unsafe xattr hardlink optimizations.
  - `CF_CHKSUM_SEED_FIX` (`1<<5`): Corrects seed order for checksums.
  - `CF_INPLACE_PARTIAL_DIR` (`1<<6`): In-place partial directory support.
  - `CF_VARINT_FLIST_FLAGS` (`1<<7`): Uses `varint` for file-list transmit flags.
  - `CF_ID0_NAMES` (`1<<8`): Explicitly transmits names for uid 0 / gid 0.

### Multiplexed I/O Channel Framing
- Upstream Files: `io.c` (`send_msg`, `read_a_msg`, `io_multiplexing_in`, `io_multiplexing_out`), `rsync.h` (`enum msgcode`, `MPLEX_BASE`).
- Framing structure:
  - Header: 4 bytes (little-endian 32-bit integer).
  - Upper 8 bits: `channel = (tag >> 24)`
  - Lower 24 bits: `length = (tag & 0xFFFFFF)`
  - `tag = ((MPLEX_BASE + (int)code) << 24) + length`, where `MPLEX_BASE = 7`.
- Message types (`enum msgcode`):
  - `MSG_DATA` (0): Normal payload data stream.
  - `MSG_ERROR_XFER` (1): Remote error diagnostics.
  - `MSG_INFO` (2): Remote progress / informational messages.
  - `MSG_ERROR` (3), `MSG_WARNING` (4): Protocol-30 diagnostic channels.
  - `MSG_REDO` (9): Generator requests Sender to reprocess file in Phase 2.
  - `MSG_STATS` (10), `MSG_BLOCK_STATS` (11): Transfer statistics.
  - `MSG_IO_ERROR` (22): Notifies receiver of sender-side I/O errors.
  - `MSG_IO_TIMEOUT` (33): Informs peer of negotiated timeout.
  - `MSG_NOOP` (42): Keepalive frame.
  - `MSG_ERROR_EXIT` (86): Clean error exit synchronization.
  - `MSG_SUCCESS` (100): Confirms successful update of flist index.
  - `MSG_DELETED` (101): Informs generator of successfully deleted file.
  - `MSG_NO_SEND` (102): Sender failed to open a requested file.

### Variable-Length Integer Encodings
- Upstream Files: `io.c` (`read_varint`, `write_varint`, `read_varlong`, `write_varlong`).
- **`varint` (up to 32-bit integer)**:
  - Compact prefix encoding where the first byte determines the extra bytes needed.
  - Extra count table `int_byte_extra[64]` based on top bits of first byte (`ch / 4`).
  - Small positive values (0 to 127) fit in 1 byte; larger values take 2, 3, 4, or 5 bytes.
- **`varlong` (64-bit integer / file offsets)**:
  - Takes `min_bytes` (e.g. 3 or 4) to optimize transmission of typical file sizes or timestamps.

---

## 2. File List (`flist`) Format and Traversal

### Tree Walking & Sorting
- Upstream Files: `flist.c` (`send_file_list`, `send_file_name`, `send_directory`, `make_file`, `f_name_cmp`), `clean_fname.c`, `sanitize_path.c`.
- Path sanitization: Collapses consecutive slashes, removes `./`, resolves internal `../` components, strips trailing slash (unless root).
- Path sorting:
  - File lists are lexicographically sorted using `f_name_cmp`.
  - In incremental recursion (`inc_recurse`), directories are sorted and traversed in sub-batches. Each directory sub-list is terminated by `NDX_FLIST_EOF` (-2) or `NDX_DONE` (-1).

### Wire Encoding of File Entries (`send_file_entry` / `receive_file_entry`)
- Each entry in the file list begins with transmission flags (`xflags`). If `CF_VARINT_FLIST_FLAGS` is negotiated, `xflags` is sent as a `varint`; otherwise a 1- or 2-byte bitfield:
  - `XMIT_TOP_DIR` (`1<<0`): Entry is a top-level directory.
  - `XMIT_SAME_MODE` (`1<<1`): File permissions/mode identical to previous entry.
  - `XMIT_EXTENDED_FLAGS` (`1<<2`): More flag bits follow.
  - `XMIT_SAME_UID` (`1<<3`), `XMIT_SAME_GID` (`1<<4`): Uid / Gid matches previous entry.
  - `XMIT_SAME_NAME` (`1<<5`): Name shares a prefix with previous file name (`l1` shared prefix length byte).
  - `XMIT_LONG_NAME` (`1<<6`): Suffix length `l2` exceeds 255 bytes (sent as `varint30`).
  - `XMIT_SAME_TIME` (`1<<7`): Mtime identical to previous entry.
  - `XMIT_SAME_RDEV_MAJOR` (`1<<8`): Major device number identical to previous entry.
  - `XMIT_NO_CONTENT_DIR` (`1<<8`): Directory without recursion content.
  - `XMIT_HLINKED` (`1<<9`): File is part of a hardlink cluster.
  - `XMIT_USER_NAME_FOLLOWS` (`1<<10`), `XMIT_GROUP_NAME_FOLLOWS` (`1<<11`): String name follows entry in incremental recursion.
  - `XMIT_HLINK_FIRST` (`1<<12`): First encounter of this dev/inode cluster.
  - `XMIT_IO_ERROR_ENDLIST` (`1<<12`): Signals directory scan hit I/O error.
  - `XMIT_MOD_NSEC` (`1<<13`): Nanosecond component of mtime follows.
  - `XMIT_SAME_ATIME` (`1<<14`): Access time identical to previous entry.
  - `XMIT_CRTIME_EQ_MTIME` (`1<<17`): Creation time equals modification time.
- Entry Payload Order:
  1. `xflags` (`varint` or `byte`/`short`).
  2. If `XMIT_SAME_NAME`: `l1` (1 byte, prefix length to retain).
  3. `l2` (suffix length, 1 byte or `varint30` if `XMIT_LONG_NAME`).
  4. File name suffix bytes (`l2` bytes).
  5. File size (`varlong30`, minimum 3 bytes).
  6. Mtime (if not `XMIT_SAME_TIME`: `varlong` or `int32`).
  7. Nanoseconds (if `XMIT_MOD_NSEC`: `varint`).
  8. Mode (if not `XMIT_SAME_MODE`: wire mode `int32`).
  9. Uid / User name (if preserved and not `XMIT_SAME_UID`).
  10. Gid / Group name (if preserved and not `XMIT_SAME_GID`).
  11. Device major/minor (if device or special).
  12. Symlink target string (if symlink).
  13. Hardlink index (if `XMIT_HLINKED`).
  14. Full-file checksum (if `-c, --checksum` is active: 16 bytes for MD5/MD4, or xxHash).

---

## 3. Checksum and Delta-Transfer Algorithm

### Weak Rolling Checksum (`get_checksum1` & Rolling Update)
- Upstream Files: `checksum.c` (`get_checksum1`), `match.c` (`hash_search`, window rolling update).
- **Initial Calculation**:
  - Based on Mark Adler's checksum, modified by Tridgell & Mackerras with `CHAR_OFFSET = 0`:
    $$\begin{aligned}
    s_1 &= \sum_{i=0}^{n-1} (buf[i] + \text{CHAR\_OFFSET}) \pmod{2^{16}} \\
    s_2 &= \sum_{i=0}^{n-1} (n - i)(buf[i] + \text{CHAR\_OFFSET}) \pmod{2^{16}} \\
    \text{checksum1} &= (s_1 \ \& \ 0\text{xFFFF}) + (s_2 \ll 16)
    \end{aligned}$$
- **Window Sliding (1-Byte Step)**:
  - When sliding a window of length $k$ from offset $X$ to $X+1$ by discarding byte $X_0$ and adding byte $X_k$:
    $$\begin{aligned}
    s_1' &= s_1 - X_0 + X_k \\
    s_2' &= s_2 - k \cdot X_0 + s_1'
    \end{aligned}$$
  - This allows updating the weak checksum in $O(1)$ operations per byte.

### Strong Checksums (`get_checksum2`)
- Upstream Files: `checksum.c`, `lib/mdfour.c`, `lib/md5.c`.
- Seed handling:
  - Sender and receiver exchange a random 32-bit `checksum_seed`.
  - In Protocol >= 30, the seed is mixed into strong checksum computations to prevent hash collision spoofing.
  - Algorithms: MD4, MD5, SHA1, and modern XXH64 / XXH3 (64-bit and 128-bit).

### Block Size and Strong Checksum Length Heuristics
- Upstream File: `generator.c` (`sum_sizes_sqroot`).
- Heuristic:
  - Block length $S = \max(\text{BLOCK\_SIZE}, \lfloor\sqrt{file\_size}\rfloor)$, rounded to a multiple of 8, capped at $2^{17} = 131,072$ bytes.
  - Strong checksum truncated length: In Phase 1, strong checksums are truncated (e.g. 2 to 8 bytes) to minimize bandwidth.
  - In Phase 2 redo (`MSG_REDO`), strong checksums are expanded to full length (16 bytes) if a whole-file verification error occurs.

### The 4-Stage Delta-Transfer Pipeline
1. **Generator (`generator.c`)**:
   - Reads existing destination file (basis).
   - Generates block checksum struct: `(count, blength, s2length, remainder)`.
   - Sends each block's `(sum1, sum2)` to the Sender.
2. **Sender (`sender.c`, `match.c`)**:
   - Receives block checksums and constructs a 16-bit hash table (`hash_table[SUM2HASH(sum1)]`).
   - Chains collisions with `MAX_CHAIN_LEN = 1024` cap (prevents $O(N^2)$ collision flood DoS).
   - Duplicate unlinking (`issue #1115`): If consecutive candidate entries in a chain share identical `sum2` with a preceding false alarm, unlinks them immediately without consuming `MAX_CHAIN_LEN` budget or incrementing `false_alarms`.
   - Collision skip heuristic: When 4 consecutive false offsets occur on strong checksum comparison (or chain exceeds `MAX_CHAIN_LEN`), skips forward `min(k - 1, end - 1 - offset)` bytes, recomputes rolling checksum at that landing offset, and then advances by 1 byte.
   - Slides rolling checksum window over the source file.
   - On match failure: slides forward 1 byte, accumulating literal data.
   - On match hit: verifies `sum2`. If valid, transmits pending literal buffer, then emits block token `-(block_index + 1)`.
   - Sends terminal token `0` followed by the whole-file digest.
3. **Receiver (`receiver.c`, `token.c`)**:
   - Reads tokens from Sender:
     - Positive value $N$: $N$ literal bytes follow.
     - Negative value $-i$: Copy block $(i - 1)$ from basis file.
     - Zero ($0$): End of file.
   - Reconstructs file into a staging temp file (`.~tmp~` or `.ssync-tmp...`).
   - Computes full-file checksum during reconstruction and compares with Sender's digest.
   - Atomically renames staging file over destination upon validation.

---

## 4. Option Semantics, Defaults, and Flag Interactions

### Composite Flag Expansions
- `-a, --archive` expands to `-rlptgoD` (`-r` recursive, `-l` symlinks, `-p` perms, `-t` times, `-g` group, `-o` owner, `-D` devices+specials).
- `-P` expands to `--partial --progress`.
- `-D` expands to `--devices --specials`.
- `-F` adds `:C` (dir-merge `.cvsignore`) on first use, and `-C` (CVS exclude) on second use.

### Quick-Check Comparison Semantics (`generator.c` `quick_check_ok`)
- Default check: File is skipped if `destination.st_size == source.length` AND `mtime_differs(dst, src) == false`.
- Timestamp comparison (`same_time` in `util1.c`):
  - When `--modify-window == 0` (default): Compares integer seconds only (`f1_sec == f2_sec`). Nanoseconds are ignored.
  - When `--modify-window < 0`: Compares exact nanoseconds (`f1_sec == f2_sec && f1_nsec == f2_nsec`).
  - When `--modify-window > 0`: Tolerates timestamp differences within $\pm W$ seconds (`|f1_sec - f2_sec| <= W`).
- If `-c, --checksum`: Size is checked; if equal, destination file is read and its full strong digest compared with `F_SUM(src)`.
- If `--size-only`: File is considered unchanged if `st_size` matches, ignoring mtime.
- If `-I, --ignore-times`: Quick check is disabled; every file is subjected to delta transfer.
- If `-u, --update`: Destination file is skipped if its mtime is newer than source. Non-regular destination files (e.g. symlinks) are never skipped by `-u`.
- Single-file destination semantics: When syncing a single source file and the destination path exists as a regular file or does not end with `/` and is not an existing directory, the destination is treated as a target file rather than a target directory.

### Server Command Line Construction (`options.c` `server_options`, `maybe_add_e_option`)
- When launching remote server via SSH, the client constructs a compact argument vector:
  - First arguments: `--server` (and `--sender` if client is pulling).
  - Compact option cluster: e.g. `-vlogDtprze.iLsfxCIvu`.
  - Capability string `-e.<flags>` informs server of client-side support for varints, symlink times, safe flist, etc.

---

## 5. Filter, Include/Exclude, and Wildmatch Rules

### Rule Representation (`rsync.h`, `exclude.c`)
- Filter rules are stored in a linked list of `filter_rule`:
  - `+` (include), `-` (exclude).
  - `!` (clear list: discards prior rules in current scope).
  - `:` (dir-merge file, e.g. `.rsync-filter`).
  - `.` (merge file).
  - `P` (protect file from deletion).
  - `R` (risk: allow deletion).
  - `H` (hide: exclude on sender, invisible to receiver).
  - `S` (show: show hidden files).

### Pattern Matching Grammar (`lib/wildmatch.c`)
- First-matching rule determines outcome. If no rule matches, file is included.
- `*`: Matches any character except path separator `/`.
- `**`: Matches any character including `/`.
- `***`: In include/exclude patterns, matches file and causes recursive inclusion of whole subtree.
- Anchoring:
  - Pattern with leading `/`: Anchored to the root of the transfer.
  - Pattern with `/` anywhere in middle: Anchored relative to the directory containing the filter rule.
  - Pattern without `/`: Matches the basename of the file at any depth.
- Trailing `/`: Matches directories only.

---

## 6. Metadata Handling, Links, Destination Modes, and Security

### Metadata Finalization Timing
- Files: Permissions, ownership, and timestamps are applied immediately after reconstruction.
- Directories: Restrictive permissions (e.g. read-only `0555`) or directory mtimes must NOT be set when creating the directory; otherwise children cannot be written, or child creation updates the parent's mtime.
- Upstream solution (`generator.c` `touch_up_dirs`): Directory attributes are pushed onto a stack and finalized in reverse topological order (deepest directories first) after all file transfers complete.

### Link Handling
- Symlinks (`-l`): Target string stored in flist; re-created with `symlink(2)`.
  - Safe links (`--safe-links`): Discards symlinks pointing outside the replicated tree root or using absolute targets.
- Hardlinks (`-H`, `hlink.c`):
  - Tracked via `(st_dev, st_ino)` hash table.
  - First occurrence is sent as a full file (`XMIT_HLINK_FIRST`).
  - Subsequent occurrences transmit only a hardlink index referencing the first file.
  - Receiver links them using `link(2)`.

### File Update & Staging Modes
- Default: Atomic staging into temporary file `.<name>.<random>` in the same directory, followed by `rename(2)` over target.
- `--inplace`: Writes delta directly into destination file using `pwrite(2)`. Avoids double disk usage; not atomic.
- `--partial`: Keeps partially transferred files if transfer is aborted.
- `--partial-dir=DIR`: Moves incomplete partial transfers into a designated staging folder.
- `--delay-updates`: Stages all modified files into `.~tmp~` subdirectories throughout the transfer, executing all renames in a rapid atomic burst at the very end.
- `-b, --backup`: Renames existing destination file to `<name>~` (or `--suffix`) or moves it to `--backup-dir`.

---

## 7. Error Codes, Exit Codes, Logging, Itemize, and Edge Cases

### Standard Exit Codes (`errcode.h`)
- `0` (`RERR_OK`): Success.
- `1` (`RERR_SYNTAX`): Syntax or command-line usage error.
- `2` (`RERR_PROTOCOL`): Protocol incompatibility or wire parse violation.
- `3` (`RERR_FILESELECT`): Errors selecting input/output files or directories.
- `4` (`RERR_UNSUPPORTED`): Requested action not supported.
- `5` (`RERR_STARTCLIENT`): Error starting client-server handshake.
- `10` (`RERR_SOCKETIO`): Error in socket/network I/O.
- `11` (`RERR_FILEIO`): Error in local file I/O.
- `12` (`RERR_STREAMIO`): Error in rsync protocol stream framing.
- `22` (`RERR_MALLOC`): Out of memory.
- `23` (`RERR_PARTIAL`): Partial transfer (some files failed to transfer).
- `24` (`RERR_VANISHED`): Source file vanished before transfer completed.
- `25` (`RERR_DEL_LIMIT`): Deletion limit (`--max-delete`) reached.
- `30` (`RERR_TIMEOUT`): I/O idle timeout expired.

### Itemize Changes (`-i, --itemize-changes`) Format
Outputs an 11-character string describing each item's modification:
1. `Y` (Update type):
   - `<`: Sent to remote.
   - `>`: Received from remote.
   - `c`: Local change (creation / modification).
   - `h`: Hardlink created.
   - `.`: Not updated (metadata attributes changed only).
   - `*`: Deleting.
2. `X` (File type):
   - `f`: Regular file.
   - `d`: Directory.
   - `L`: Symlink.
   - `D`: Device.
   - `S`: Special file (FIFO/socket).
3. `c` (Checksum/content): `c` if content changed, `+` for newly created file, `.` if identical.
4. `s` (Size): `s` if file size changed.
5. `t` (Time): `t` if mtime changed, `T` if mtime could not be set.
6. `p` (Permissions): `p` if mode changed.
7. `o` (Owner): `o` if uid changed.
8. `g` (Group): `g` if gid changed.
9. `u` (Access time): `u` if atime changed, `n` for crtime, `b` for both.
10. `a` (ACL): `a` if ACL changed.
11. `x` (Extended attributes): `x` if xattr changed.

### Tricky Edge Cases
- **Trailing Slash on Source**:
  - `ssync src dest` creates `dest/src/...` (transfers the directory itself).
  - `ssync src/ dest` creates `dest/...` (transfers the contents of `src` into `dest`).
- **Vanishing Source Files**:
  - If a file exists during initial tree scan but is deleted before the sender opens it, rsync emits `MSG_NO_SEND`, logs a warning, skips the file, and exits with code `24` (`RERR_VANISHED`) instead of terminating the whole run.
- **Replacing Directory with File (and vice versa)**:
  - Requires `--force` to delete a non-empty directory when replacing it with a file, symlink, or vice versa.
- **Unsafe Symlink Detection (`unsafe_symlink` in `util1.c`)**:
  - Symlinks with absolute targets (starting with `/`) or null targets are always unsafe.
  - Targets with `..` components other than leading `../` (e.g. `a/a/a/../../../unsafe` or `foo/../bar`) are always rejected as unsafe because intermediate directory components could later become symlinks, leading to an escape.
  - For clean targets with only leading `../`, the target is unsafe if the count of leading `../` components strictly exceeds the directory depth of the symlink from the transfer root.
