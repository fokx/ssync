//! Wire protocol client and server execution for remote shell transfers.

use anyhow::{Context, Result, bail};
use ssync_core::checksum::{ChecksumAlgorithm, RollingChecksum, calculate_strong_checksum};
use ssync_core::delta::{BlockSignature, DeltaToken, SumStruct, generate_delta};
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
    list_only: bool,
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

    if let Some(ff) = files_from {
        if let Some(stripped) = ff.strip_prefix(':') {
            cmd.arg(format!("--files-from={}", stripped));
        } else {
            cmd.arg("--files-from=-");
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

    if is_sender {
        // Client is SENDER, remote server is RECEIVER
        wire_send(
            sources_raw,
            &mut reader,
            &mut writer,
            false,
            itemize,
            files_from,
        )?;
    } else {
        // Client is RECEIVER, remote server is SENDER
        let local_dest = Path::new(dest_raw.trim_end_matches('/'));
        wire_recv(
            local_dest,
            &mut reader,
            &mut writer,
            false,
            delete,
            itemize,
            perms,
            times,
            list_only,
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
pub fn run_wire_server(
    is_sender: bool,
    protect_args: bool,
    delete: bool,
    paths: &[String],
    itemize: bool,
    perms: bool,
    times: bool,
    files_from: Option<&str>,
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

    if is_sender {
        let targets = if effective_paths.len() > 1 {
            &effective_paths[1..]
        } else {
            &effective_paths[..]
        };
        wire_send(targets, &mut reader, &mut writer, true, itemize, files_from)?;
    } else {
        let target_dir = if effective_paths.len() > 1 {
            Path::new(&effective_paths[1])
        } else {
            Path::new(".")
        };
        wire_recv(
            target_dir,
            &mut reader,
            &mut writer,
            true,
            delete,
            itemize,
            perms,
            times,
            false,
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

fn wire_send<R: Read, W: Write>(
    sources: &[String],
    reader: &mut MplexReader<R>,
    writer: &mut MplexWriter<W>,
    _is_server: bool,
    _itemize: bool,
    files_from: Option<&str>,
) -> Result<()> {
    // 1. Send empty filter list
    write_int(writer, 0)?;
    writer.flush()?;

    // 2. Scan and build file list using Walker
    let walk_opts = WalkOptions {
        recursive: true,
        dirs: true,
        links: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let source_paths: Vec<SourcePath> = sources.iter().map(|s| SourcePath::from_raw(s)).collect();
    let transfer_entries = if let Some(ff) = files_from {
        if !source_paths.is_empty() {
            walker.scan_files_from(&source_paths[0], ff, false)?
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
            FileType::Directory => 0o040000 | te.metadata.mode,
            FileType::Symlink => 0o120000 | te.metadata.mode,
            _ => 0o100000 | te.metadata.mode,
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
    reader: &mut MplexReader<R>,
    writer: &mut MplexWriter<W>,
    is_server: bool,
    delete: bool,
    itemize: bool,
    perms: bool,
    times: bool,
    list_only: bool,
) -> Result<()> {
    // 1. Read filter list (0 means empty)
    let filter_len = read_int(reader)?;
    if filter_len != 0 {
        let mut discard = vec![0u8; filter_len as usize];
        reader.read_exact(&mut discard)?;
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

    if !dest_dir.exists() {
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
                if p.is_dir() && !p.is_symlink() {
                    let _ = fs::remove_dir(&p);
                } else {
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

    // 3. Process each entry
    for entry in &entries {
        let is_root = entry.name.is_empty() || entry.name == ".";
        let target_path = if is_root {
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
            if let Some(ref target) = entry.symlink_target {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let _ = fs::remove_file(&target_path);
                std::os::unix::fs::symlink(target, &target_path)?;
                if itemize {
                    report_info(
                        is_server,
                        writer,
                        &format!("cL+++++++++ {} -> {}\n", entry.name, target),
                    )?;
                }
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
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
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

                fs::write(&target_path, &reconstructed)?;
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

                fs::write(&target_path, &reconstructed)?;
            }

            if itemize {
                report_info(is_server, writer, &format!("<f+++++++++ {}\n", entry.name))?;
            }
        }

        // Apply metadata to non-directory file
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
