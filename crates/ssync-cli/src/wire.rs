//! Wire protocol client and server execution for remote shell transfers.

use anyhow::{Context, Result, bail};
use ssync_core::checksum::{ChecksumAlgorithm, RollingChecksum, calculate_strong_checksum};
use ssync_core::delta::{BlockSignature, DeltaToken, SumStruct, generate_delta};
use ssync_fs::filter::FilterSet;
use ssync_fs::metadata::{FileMetadata, FileType};
use ssync_fs::path::SourcePath;
use ssync_fs::walker::{WalkOptions, Walker};
use ssync_proto::flist::{WireFileEntry, recv_wire_file_list, send_wire_file_list};
use ssync_proto::handshake::{
    CF_CHKSUM_SEED_FIX, CF_SAFE_FLIST, client_handshake, server_handshake,
};
use ssync_proto::mplex::{MplexReader, MplexWriter, MsgCode};
use ssync_proto::transfer::{
    WireBlockSum, recv_block_checksums, recv_delta_tokens, send_block_checksums, send_delta_tokens,
};
use ssync_proto::varint::{read_int, write_int};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SHELL_CHARS: &str = "!#$&;|<>(){}\"'` \t\n\r\\";

/// Escapes shell characters in a command argument unless `old_args` is set.
pub fn safe_arg(arg: &str, old_args: bool) -> String {
    if old_args {
        return arg.to_string();
    }
    let mut out = String::new();
    for ch in arg.chars() {
        if SHELL_CHARS.contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Parses a path to check if it refers to a remote machine (`[user@]host:path`).
pub fn parse_remote_path(raw: &str) -> Option<(Option<String>, String, String)> {
    if raw.starts_with('/')
        || raw.starts_with("./")
        || raw.starts_with("../")
        || raw.starts_with(':')
    {
        return None;
    }
    let colon_pos = raw.find(':')?;
    let host_part = &raw[..colon_pos];
    if host_part.contains('/') || host_part.is_empty() {
        return None;
    }
    let remote_path = raw[colon_pos + 1..].to_string();
    if let Some(at_pos) = host_part.find('@') {
        let user = host_part[..at_pos].to_string();
        let host = host_part[at_pos + 1..].to_string();
        Some((Some(user), host, remote_path))
    } else {
        Some((None, host_part.to_string(), remote_path))
    }
}

/// Connects to a remote shell process and executes the wire transfer loop as client.
#[allow(clippy::too_many_arguments)]
pub fn run_wire_client(
    sources_raw: &[String],
    dest_raw: &str,
    rsh_override: Option<&str>,
    rsync_path_override: Option<&str>,
    protect_args: bool,
    old_args: bool,
    delete: bool,
    itemize: bool,
    perms: bool,
    times: bool,
    max_alloc: Option<&str>,
    remote_options: &[String],
    files_from: Option<&str>,
    from0: bool,
    list_only: bool,
    backup: bool,
    backup_dir: Option<&str>,
    partial: bool,
    partial_dir: Option<&str>,
    inplace: bool,
    append: bool,
    delay_updates: bool,
    temp_dir: Option<&str>,
    link_dest: &[String],
    copy_dest: &[String],
    compare_dest: &[String],
    relative: bool,
    filter_rules: &[String],
    filters: &FilterSet,
    confine_root: Option<&Path>,
    ignore_existing: bool,
    delay_symlinks: bool,
    suffix: Option<&str>,
) -> Result<()> {
    // Determine remote side
    let dest_remote = parse_remote_path(dest_raw);
    let src_remote = if !sources_raw.is_empty() {
        parse_remote_path(&sources_raw[0])
    } else {
        None
    };

    let (is_sender, remote_user, remote_host, remote_files) = if let Some((u, h, p)) = dest_remote {
        (true, u, h, vec![p])
    } else if let Some((u, h, _)) = src_remote {
        let mut paths = Vec::new();
        for s in sources_raw {
            if let Some((_, _, p)) = parse_remote_path(s) {
                paths.push(p);
            } else {
                paths.push(s.clone());
            }
        }
        (false, u, h, paths)
    } else {
        bail!("run_wire_client called without any remote path");
    };

    let rsh_env = std::env::var("RSYNC_RSH").ok();
    let rsh_cmd = rsh_override.or(rsh_env.as_deref()).unwrap_or("ssh");

    let mut cmd_tokens = rsh_cmd.split_whitespace().collect::<Vec<_>>();
    let prog = cmd_tokens.remove(0);

    let mut cmd = Command::new(prog);
    for t in cmd_tokens {
        cmd.arg(t);
    }
    if let Some(ref u) = remote_user {
        cmd.arg("-l").arg(u);
    }
    cmd.arg(&remote_host);

    let rsync_binary = rsync_path_override.unwrap_or("rsync");
    cmd.arg(rsync_binary);
    cmd.arg("--server");
    if !is_sender {
        cmd.arg("--sender");
    }

    let mut opts_str = String::from("-");
    if protect_args {
        opts_str.push('s');
    }
    opts_str.push_str("lptgoDr");
    if relative {
        opts_str.push('R');
    }
    if itemize {
        opts_str.push('i');
    }
    cmd.arg(&opts_str);

    if delete {
        cmd.arg("--delete");
    }

    if list_only {
        cmd.arg("--list-only");
    }

    if backup {
        cmd.arg("-b");
    }
    if let Some(s) = suffix {
        cmd.arg(format!("--suffix={}", s));
    }
    if let Some(bd) = backup_dir {
        cmd.arg(format!("--backup-dir={}", bd));
    }
    if ignore_existing {
        cmd.arg("--ignore-existing");
    }
    if delay_symlinks {
        cmd.arg("--delay-symlinks");
    }
    if partial {
        cmd.arg("--partial");
    }
    if let Some(pd) = partial_dir {
        cmd.arg(format!("--partial-dir={}", pd));
    }
    if inplace {
        cmd.arg("--inplace");
    }
    if append {
        cmd.arg("--append");
    }
    if delay_updates {
        cmd.arg("--delay-updates");
    }
    if let Some(td) = temp_dir {
        cmd.arg(format!("--temp-dir={}", td));
    }
    for ld in link_dest {
        cmd.arg(format!("--link-dest={}", ld));
    }
    for cd in copy_dest {
        cmd.arg(format!("--copy-dest={}", cd));
    }
    for cmp in compare_dest {
        cmd.arg(format!("--compare-dest={}", cmp));
    }

    if let Some(ff) = files_from {
        if let Some(stripped) = ff.strip_prefix(':') {
            cmd.arg(format!("--files-from={}", stripped));
            if from0 {
                cmd.arg("--from0");
            }
        } else if !is_sender {
            cmd.arg("--files-from=-");
            cmd.arg("--from0");
        }
    }

    for opt in remote_options {
        cmd.arg(opt);
    }

    if let Some(m) = max_alloc {
        cmd.arg(format!("--max-alloc={}", m));
    }

    if !protect_args {
        cmd.arg(".");
        for rf in &remote_files {
            cmd.arg(safe_arg(rf, old_args));
        }
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn remote shell: {}", rsh_cmd))?;

    let mut stdin = child.stdin.take().expect("Child stdin unavailable");
    let mut stdout = child.stdout.take().expect("Child stdout unavailable");

    // If protect_args is active, send args null-terminated over stdin
    if protect_args {
        stdin.write_all(b"rsync\0")?;
        stdin.write_all(b".\0")?;
        for rf in &remote_files {
            stdin.write_all(rf.as_bytes())?;
            stdin.write_all(b"\0")?;
        }
        stdin.write_all(b"\0")?;
        stdin.flush()?;
    }

    // Protocol Handshake
    let _handshake = client_handshake(&mut stdout, &mut stdin)
        .context("Client handshake failed with remote server")?;

    let mut reader = MplexReader::new(stdout);
    let mut writer = MplexWriter::new(stdin);

    // Client sends filter list to server
    for rule in filter_rules {
        write_int(&mut writer, rule.len() as i32)?;
        writer.write_all(rule.as_bytes())?;
    }
    write_int(&mut writer, 0)?;
    writer.flush()?;

    if is_sender {
        // Client is SENDER, remote server is RECEIVER
        wire_send(
            sources_raw,
            &mut reader,
            &mut writer,
            false,
            itemize,
            files_from,
            from0,
            list_only,
            relative,
            filters,
            confine_root,
        )?;
    } else {
        // Client is RECEIVER, remote server is SENDER
        let local_dest = Path::new(dest_raw.trim_end_matches('/'));
        let bd_path = backup_dir.map(Path::new);
        wire_recv(
            local_dest,
            dest_raw,
            &mut reader,
            &mut writer,
            false,
            delete,
            itemize,
            perms,
            times,
            list_only,
            backup,
            bd_path,
            suffix.unwrap_or("~"),
            link_dest,
            files_from,
            from0,
            ignore_existing,
            delay_symlinks,
        )?;
    }

    let status = child
        .wait()
        .context("Failed waiting on remote shell child process")?;
    if !status.success() {
        bail!("Remote server process exited with status: {}", status);
    }

    Ok(())
}

/// Runs ssync in server mode (`--server`).
#[allow(clippy::too_many_arguments)]
pub fn run_wire_server(
    is_sender: bool,
    protect_args: bool,
    delete: bool,
    paths: &[String],
    itemize: bool,
    perms: bool,
    times: bool,
    files_from: Option<&str>,
    from0: bool,
    backup: bool,
    backup_dir: Option<PathBuf>,
    suffix: String,
    list_only: bool,
    relative: bool,
    link_dest: &[String],
    confine_root: Option<&Path>,
    ignore_existing: bool,
    delay_symlinks: bool,
) -> Result<()> {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let mut effective_paths = paths.to_vec();

    // If protect_args is active, read null-separated arguments from stdin
    if protect_args {
        let mut raw_args = Vec::new();
        let mut buf = [0u8; 1];
        let mut cur = Vec::new();
        loop {
            stdin.read_exact(&mut buf)?;
            if buf[0] == 0 {
                if cur.is_empty() {
                    break;
                }
                raw_args.push(String::from_utf8_lossy(&cur).to_string());
                cur.clear();
            } else {
                cur.push(buf[0]);
            }
        }
        if !raw_args.is_empty() {
            // Skip program name ("rsync")
            effective_paths = raw_args.into_iter().skip(1).collect();
        }
    }

    if !effective_paths.is_empty() {
        let dir = &effective_paths[0];
        if dir != "." {
            let _ = std::env::set_current_dir(dir);
        }
    }

    let compat_flags = CF_SAFE_FLIST | CF_CHKSUM_SEED_FIX;
    let seed = 12345;
    let _handshake = server_handshake(&mut stdin, &mut stdout, compat_flags, seed)
        .context("Server handshake failed")?;

    let mut reader = MplexReader::new(stdin);
    let mut writer = MplexWriter::new(stdout);

    // Server receives filter list from client right after handshake
    let mut filter_set = FilterSet::new();
    filter_set.confine_root = confine_root.map(|p| p.to_path_buf());
    loop {
        let len = read_int(&mut reader)?;
        if len == 0 {
            break;
        }
        let mut buf = vec![0u8; len as usize];
        reader.read_exact(&mut buf)?;
        let rule_str = String::from_utf8_lossy(&buf);
        let _ = filter_set.add_filter_arg(&rule_str);
    }

    if is_sender {
        let targets = if effective_paths.len() > 1 {
            &effective_paths[1..]
        } else {
            &effective_paths[..]
        };
        wire_send(
            targets,
            &mut reader,
            &mut writer,
            true,
            itemize,
            files_from,
            from0,
            list_only,
            relative,
            &filter_set,
            confine_root,
        )?;
    } else {
        let (target_dir, dest_raw) = if effective_paths.len() > 1 {
            (Path::new(&effective_paths[1]), effective_paths[1].as_str())
        } else {
            (Path::new("."), ".")
        };
        wire_recv(
            target_dir,
            dest_raw,
            &mut reader,
            &mut writer,
            true,
            delete,
            itemize,
            perms,
            times,
            false,
            backup,
            backup_dir.as_deref(),
            &suffix,
            link_dest,
            files_from,
            from0,
            ignore_existing,
            delay_symlinks,
        )?;
    }

    Ok(())
}

fn report_info<W: Write>(is_server: bool, writer: &mut MplexWriter<W>, msg: &str) -> Result<()> {
    if is_server {
        writer.send_msg(MsgCode::Info, msg.as_bytes())?;
    } else {
        print!("{}", msg);
        let _ = std::io::stdout().flush();
    }
    Ok(())
}

fn create_symlink<W: Write>(
    target_path: &Path,
    entry: &WireFileEntry,
    is_server: bool,
    writer: &mut MplexWriter<W>,
    itemize: bool,
    perms: bool,
    times: bool,
) -> Result<()> {
    if let Some(ref target) = entry.symlink_target {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Ok(meta) = target_path.symlink_metadata() {
            if meta.is_dir() {
                let _ = fs::remove_dir(target_path);
            } else {
                let _ = fs::remove_file(target_path);
            }
        }
        std::os::unix::fs::symlink(target, target_path)?;
        if itemize {
            report_info(
                is_server,
                writer,
                &format!("cL+++++++++ {} -> {}\n", entry.name, target),
            )?;
        }
    }
    if let Ok(mut fs_meta) = FileMetadata::from_path(target_path, false) {
        if perms {
            fs_meta.mode = entry.mode;
        }
        if times {
            fs_meta.mtime_sec = entry.mtime_secs;
            fs_meta.mtime_nsec = entry.mtime_nanos as i64;
        }
        let _ = fs_meta.apply_to(target_path, perms, times, false, false);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn wire_send<R: Read, W: Write>(
    sources: &[String],
    reader: &mut MplexReader<R>,
    writer: &mut MplexWriter<W>,
    is_server: bool,
    _itemize: bool,
    files_from: Option<&str>,
    from0: bool,
    list_only: bool,
    relative: bool,
    filters: &FilterSet,
    confine_root: Option<&Path>,
) -> Result<()> {
    // Scan and build file list using Walker
    let walk_opts = WalkOptions {
        recursive: true,
        dirs: true,
        links: true,
        relative,
        filters: filters.clone(),
        confine_root: confine_root.map(|p| p.to_path_buf()),
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let source_paths: Vec<SourcePath> = sources.iter().map(|s| SourcePath::from_raw(s)).collect();
    let transfer_entries = if let Some(ff) = files_from {
        if ff == "-" && is_server {
            // Read null-terminated filenames from wire reader
            let mut wire_lines = Vec::new();
            let mut cur_buf = Vec::new();
            let mut byte_buf = [0u8; 1];
            loop {
                reader.read_exact(&mut byte_buf)?;
                if byte_buf[0] == 0 {
                    if cur_buf.is_empty() {
                        break;
                    }
                    wire_lines.push(String::from_utf8_lossy(&cur_buf).to_string());
                    cur_buf.clear();
                } else {
                    cur_buf.push(byte_buf[0]);
                }
            }
            let src = if !source_paths.is_empty() {
                &source_paths[0]
            } else {
                &SourcePath::from_raw(".")
            };
            walker.scan_from_lines(src, &wire_lines)?
        } else if !source_paths.is_empty() {
            walker.scan_files_from(&source_paths[0], ff, from0)?
        } else {
            walker.scan_sources(&source_paths)?
        }
    } else {
        walker.scan_sources(&source_paths)?
    };

    let mut entries = Vec::new();
    let mut file_payloads = Vec::new();

    for te in &transfer_entries {
        let is_symlink = te.metadata.file_type == FileType::Symlink;
        let symlink_target = if is_symlink {
            te.metadata
                .symlink_target
                .clone()
                .map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };

        let raw_mode = match te.metadata.file_type {
            FileType::Directory => 0o040000 | (te.metadata.mode & 0o7777),
            FileType::Symlink => 0o120000 | (te.metadata.mode & 0o7777),
            FileType::Fifo => 0o010000 | (te.metadata.mode & 0o7777),
            FileType::Socket => 0o140000 | (te.metadata.mode & 0o7777),
            FileType::BlockDevice => 0o060000 | (te.metadata.mode & 0o7777),
            FileType::CharDevice => 0o020000 | (te.metadata.mode & 0o7777),
            _ => 0o100000 | (te.metadata.mode & 0o7777),
        };

        entries.push(WireFileEntry {
            name: te.relative_dest_path.to_string_lossy().to_string(),
            size: te.metadata.size,
            mtime_secs: te.metadata.mtime_sec,
            mtime_nanos: te.metadata.mtime_nsec.max(0) as u32,
            mode: raw_mode,
            uid: te.metadata.uid,
            gid: te.metadata.gid,
            symlink_target,
        });

        if te.metadata.file_type == FileType::Regular {
            let content = fs::read(&te.source_path)?;
            file_payloads.push(Some(content));
        } else {
            file_payloads.push(None);
        }
    }

    // Send file list
    send_wire_file_list(writer, &entries)?;

    if list_only {
        return Ok(());
    }

    // Transfer regular files
    for (_entry, content_opt) in entries.iter().zip(file_payloads.iter()) {
        if let Some(content) = content_opt {
            // Read basis block checksums from receiver
            let (sum_struct, wire_sums) = recv_block_checksums(reader)?;

            let whole_file_md5 = calculate_strong_checksum(ChecksumAlgorithm::Md5, content, None);

            if sum_struct.count == 0 {
                // Whole file transfer
                send_delta_tokens(
                    writer,
                    &[DeltaToken::Literal(content.clone())],
                    &whole_file_md5,
                )?;
            } else {
                // Compute deltas using BlockSignature
                let mut sigs = Vec::with_capacity(wire_sums.len());
                let mut offset = 0u64;
                for (i, ws) in wire_sums.iter().enumerate() {
                    let block_len = if i == wire_sums.len() - 1 && sum_struct.remainder > 0 {
                        sum_struct.remainder
                    } else {
                        sum_struct.block_len
                    };
                    sigs.push(BlockSignature {
                        index: i as u32,
                        offset,
                        len: block_len,
                        sum1: ws.sum1,
                        sum2: ws.sum2.clone(),
                    });
                    offset += block_len as u64;
                }

                let (tokens, _stats) = generate_delta(
                    content,
                    &sigs,
                    &sum_struct,
                    ChecksumAlgorithm::Md5,
                    None,
                    false,
                );

                send_delta_tokens(writer, &tokens, &whole_file_md5)?;
            }
        }
    }

    // Terminal NDX_DONE
    write_int(writer, -1)?;
    writer.flush()?;

    Ok(())
}

fn collect_dest_paths(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                collect_dest_paths(&path, out);
            }
            out.push(path);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn wire_recv<R: Read, W: Write>(
    dest_dir: &Path,
    dest_raw: &str,
    reader: &mut MplexReader<R>,
    writer: &mut MplexWriter<W>,
    is_server: bool,
    delete: bool,
    itemize: bool,
    perms: bool,
    times: bool,
    list_only: bool,
    backup: bool,
    backup_dir: Option<&Path>,
    suffix: &str,
    link_dest: &[String],
    files_from: Option<&str>,
    from0: bool,
    ignore_existing: bool,
    delay_symlinks: bool,
) -> Result<()> {
    // If client is receiver and has files_from, stream filenames null-terminated to sender
    if !is_server {
        if let Some(ff) = files_from {
            let raw_data = if ff == "-" {
                let mut buf = Vec::new();
                std::io::stdin().read_to_end(&mut buf)?;
                buf
            } else {
                std::fs::read(ff)?
            };

            let lines: Vec<String> = if from0 {
                raw_data
                    .split(|&b| b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .collect()
            } else {
                let text = String::from_utf8_lossy(&raw_data);
                text.lines().map(|s| s.to_string()).collect()
            };

            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                    continue;
                }
                writer.write_all(trimmed.as_bytes())?;
                writer.write_all(b"\0")?;
            }
            // Send empty string to terminate files-from list
            writer.write_all(b"\0")?;
            writer.flush()?;
        }
    }

    // 2. Receive file list
    let entries = recv_wire_file_list(reader)?;

    if list_only {
        for entry in &entries {
            let rel = if entry.name.is_empty() {
                "."
            } else {
                &entry.name
            };
            let file_type = if (entry.mode & 0o170000) == 0o040000 {
                FileType::Directory
            } else if (entry.mode & 0o170000) == 0o120000 {
                FileType::Symlink
            } else {
                FileType::Regular
            };
            let mode_str = ssync_fs::metadata::format_mode(entry.mode & 0o777, file_type);
            let time_str = ssync_fs::metadata::format_time(entry.mtime_secs);
            println!("{:11} {:12} {} {}", mode_str, entry.size, time_str, rel);
        }
        return Ok(());
    }

    let dest_is_file = !dest_dir.is_dir()
        && !dest_raw.ends_with('/')
        && entries.len() == 1
        && (entries[0].mode & 0o170000) != 0o040000;

    if !dest_is_file && !dest_dir.exists() {
        fs::create_dir_all(dest_dir)?;
    }

    // Deletion pass (if --delete is active)
    if delete && dest_dir.is_dir() {
        let mut transferred_set: HashSet<PathBuf> = HashSet::new();
        for entry in &entries {
            let p = if entry.name.is_empty() || entry.name == "." {
                dest_dir.to_path_buf()
            } else {
                dest_dir.join(&entry.name)
            };
            transferred_set.insert(p);
        }

        let mut on_disk = Vec::new();
        collect_dest_paths(dest_dir, &mut on_disk);
        // Delete deepest paths first
        on_disk.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

        for p in on_disk {
            if !transferred_set.contains(&p) && p != dest_dir {
                if backup && (p.is_file() || p.is_symlink()) {
                    let rel = p.strip_prefix(dest_dir).unwrap_or(&p);
                    let bak_path = if let Some(bd) = backup_dir {
                        bd.join(rel)
                    } else {
                        let mut s = p.as_os_str().to_os_string();
                        s.push(suffix);
                        PathBuf::from(s)
                    };
                    if let Some(parent) = bak_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::rename(&p, &bak_path);
                }
                if p.is_dir() && !p.is_symlink() {
                    let _ = fs::remove_dir(&p);
                } else if p.exists() || p.is_symlink() {
                    let _ = fs::remove_file(&p);
                }
                if itemize {
                    let rel = p.strip_prefix(dest_dir).unwrap_or(&p);
                    report_info(
                        is_server,
                        writer,
                        &format!("*deleting   {}\n", rel.display()),
                    )?;
                }
            }
        }
    }

    let mut dirs_to_update = Vec::new();
    let mut deferred_symlinks = Vec::new();

    // 3. Process each entry
    for entry in &entries {
        let is_root = entry.name.is_empty() || entry.name == ".";
        let target_path = if is_root || dest_is_file {
            dest_dir.to_path_buf()
        } else if dest_dir.is_dir() {
            dest_dir.join(&entry.name)
        } else {
            dest_dir.to_path_buf()
        };

        let file_type = entry.mode & 0o170000;
        if file_type == 0o040000 {
            // Directory
            let existed = target_path.exists();
            fs::create_dir_all(&target_path)?;
            if itemize && (!existed || !is_root) {
                let name = if is_root { "." } else { &entry.name };
                report_info(is_server, writer, &format!("cd+++++++++ {}/\n", name))?;
            }
            dirs_to_update.push((target_path.clone(), entry.clone()));
            continue;
        } else if file_type == 0o120000 {
            // Symlink
            if delay_symlinks {
                deferred_symlinks.push((target_path.clone(), entry.clone()));
                continue;
            }
            if ignore_existing && (target_path.exists() || target_path.is_symlink()) {
                continue;
            }
            create_symlink(
                &target_path,
                entry,
                is_server,
                writer,
                itemize,
                perms,
                times,
            )?;
            continue;
        } else if file_type == 0o010000 {
            // FIFO / named pipe
            if ignore_existing && (target_path.exists() || target_path.is_symlink()) {
                continue;
            }
            if let Some(parent) = target_path.parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            if target_path.exists() || target_path.is_symlink() {
                let _ = fs::remove_file(&target_path);
            }
            let c_path = std::ffi::CString::new(target_path.as_os_str().as_bytes()).unwrap();
            unsafe {
                libc::mkfifo(c_path.as_ptr(), (entry.mode & 0o777) as libc::mode_t);
            }
            if itemize {
                report_info(is_server, writer, &format!("cS+++++++++ {}\n", entry.name))?;
            }
            if let Ok(mut fs_meta) = FileMetadata::from_path(&target_path, false) {
                if perms {
                    fs_meta.mode = entry.mode;
                }
                if times {
                    fs_meta.mtime_sec = entry.mtime_secs;
                    fs_meta.mtime_nsec = entry.mtime_nanos as i64;
                }
                let _ = fs_meta.apply_to(&target_path, perms, times, false, false);
            }
            continue;
        } else {
            // Regular file
            let skip_update = ignore_existing && (target_path.exists() || target_path.is_symlink());

            if let Some(parent) = target_path.parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }

            if !skip_update && backup && target_path.exists() {
                let bak_path = if let Some(bd) = backup_dir {
                    bd.join(&entry.name)
                } else {
                    let mut s = target_path.as_os_str().to_os_string();
                    s.push(suffix);
                    PathBuf::from(s)
                };
                if let Some(parent) = bak_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::copy(&target_path, &bak_path) {
                    eprintln!(
                        "ssync [server]: backup to {} failed: {}",
                        bak_path.display(),
                        e
                    );
                    bail!("backup to {} failed: {}", bak_path.display(), e);
                }
            }

            // Check --link-dest candidates if target_path does not exist
            if !target_path.exists() && !link_dest.is_empty() {
                for ldir in link_dest {
                    let full_dir = if ldir.starts_with('/') {
                        PathBuf::from(ldir)
                    } else if dest_dir.join(ldir).exists() {
                        dest_dir.join(ldir)
                    } else {
                        PathBuf::from(ldir)
                    };
                    let cand = if is_root || dest_is_file {
                        full_dir.join(target_path.file_name().unwrap_or_default())
                    } else {
                        full_dir.join(&entry.name)
                    };
                    if let Ok(meta) = cand.symlink_metadata() {
                        if meta.is_file() && meta.len() == entry.size {
                            let _ = fs::hard_link(&cand, &target_path);
                            break;
                        }
                    }
                }
            }

            let basis_data = if target_path.is_file() {
                fs::read(&target_path).ok()
            } else {
                None
            };

            if let Some(basis) = basis_data {
                let sum_struct =
                    SumStruct::calculate(basis.len() as u64, None, ChecksumAlgorithm::Md5);
                let mut wire_sums = Vec::with_capacity(sum_struct.count);
                for i in 0..sum_struct.count {
                    let start = i * sum_struct.block_len;
                    let end = (start + sum_struct.block_len).min(basis.len());
                    let block = &basis[start..end];
                    let sum1 = RollingChecksum::calculate(block).value();
                    let sum2 = calculate_strong_checksum(ChecksumAlgorithm::Md5, block, None);
                    wire_sums.push(WireBlockSum { sum1, sum2 });
                }
                send_block_checksums(writer, &sum_struct, &wire_sums)?;

                let (tokens, expected_md5) = recv_delta_tokens(reader, 16)?;
                let reconstructed = reconstruct_from_tokens(&basis, &tokens, sum_struct.block_len);

                let computed_md5 =
                    calculate_strong_checksum(ChecksumAlgorithm::Md5, &reconstructed, None);
                if computed_md5 != expected_md5 {
                    bail!("MD5 mismatch for file: {}", entry.name);
                }

                if !skip_update && (reconstructed != basis || !target_path.exists()) {
                    let _ = fs::remove_file(&target_path);
                    fs::write(&target_path, &reconstructed)?;
                }
            } else {
                let sum_struct = SumStruct {
                    file_len: 0,
                    count: 0,
                    block_len: 700,
                    strong_sum_len: 16,
                    remainder: 0,
                };
                send_block_checksums(writer, &sum_struct, &[])?;

                let (tokens, expected_md5) = recv_delta_tokens(reader, 16)?;
                let mut reconstructed = Vec::new();
                for token in tokens {
                    if let DeltaToken::Literal(bytes) = token {
                        reconstructed.extend_from_slice(&bytes);
                    }
                }

                let computed_md5 =
                    calculate_strong_checksum(ChecksumAlgorithm::Md5, &reconstructed, None);
                if computed_md5 != expected_md5 {
                    bail!("MD5 mismatch for file: {}", entry.name);
                }

                if !skip_update {
                    fs::write(&target_path, &reconstructed)?;
                }
            }

            if !skip_update && itemize {
                report_info(is_server, writer, &format!("<f+++++++++ {}\n", entry.name))?;
            }

            // Apply metadata to non-directory file
            if !skip_update {
                if let Ok(mut fs_meta) = FileMetadata::from_path(&target_path, false) {
                    if perms {
                        fs_meta.mode = entry.mode;
                    }
                    if times {
                        fs_meta.mtime_sec = entry.mtime_secs;
                        fs_meta.mtime_nsec = entry.mtime_nanos as i64;
                    }
                    let _ = fs_meta.apply_to(&target_path, perms, times, false, false);
                }
            }
        }
    }

    // Create deferred symlinks
    for (t_path, e) in deferred_symlinks {
        if ignore_existing && (t_path.exists() || t_path.is_symlink()) {
            continue;
        }
        create_symlink(&t_path, &e, is_server, writer, itemize, perms, times)?;
    }

    // Apply directory metadata in reverse post-order (deepest directories first)
    dirs_to_update.sort_by_key(|(p, _)| std::cmp::Reverse(p.components().count()));
    for (dir_path, dir_entry) in dirs_to_update {
        if let Ok(mut fs_meta) = FileMetadata::from_path(&dir_path, false) {
            if perms {
                fs_meta.mode = dir_entry.mode;
            }
            if times {
                fs_meta.mtime_sec = dir_entry.mtime_secs;
                fs_meta.mtime_nsec = dir_entry.mtime_nanos as i64;
            }
            let _ = fs_meta.apply_to(&dir_path, perms, times, false, false);
        }
    }

    // Read terminal NDX_DONE
    let _term = read_int(reader)?;

    Ok(())
}

fn reconstruct_from_tokens(basis: &[u8], tokens: &[DeltaToken], block_len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for token in tokens {
        match token {
            DeltaToken::Literal(bytes) => {
                out.extend_from_slice(bytes);
            }
            DeltaToken::BlockRef(idx) => {
                let start = (*idx as usize) * block_len;
                let end = (start + block_len).min(basis.len());
                if start < basis.len() {
                    out.extend_from_slice(&basis[start..end]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remote_path() {
        assert_eq!(
            parse_remote_path("user@host:path/to/file"),
            Some((
                Some("user".to_string()),
                "host".to_string(),
                "path/to/file".to_string()
            ))
        );
        assert_eq!(
            parse_remote_path("host:path/to/file"),
            Some((None, "host".to_string(), "path/to/file".to_string()))
        );
        assert_eq!(parse_remote_path("/local/path/to/file"), None);
    }

    #[test]
    fn test_safe_arg() {
        assert_eq!(safe_arg("plain_path", false), "plain_path");
        assert_eq!(safe_arg("path with spaces", false), "path\\ with\\ spaces");
        assert_eq!(safe_arg("path with spaces", true), "path with spaces");
    }

    #[test]
    fn test_file_type_mode_encoding() {
        let entry_reg = 0o100000 | 0o644;
        let entry_dir = 0o040000 | 0o755;
        let entry_fifo = 0o010000 | 0o644;
        assert_eq!(entry_reg & 0o170000, 0o100000);
        assert_eq!(entry_dir & 0o170000, 0o040000);
        assert_eq!(entry_fifo & 0o170000, 0o010000);
    }
}
