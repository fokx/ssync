//! Synchronization engine executing local filesystem transfers.

use crate::FsError;
use crate::filter::FilterSet;
use crate::metadata::{FileMetadata, FileType};
use crate::secure::{safe_create_dir_all, safe_open_source, safe_rename_into};
use crate::walker::TransferEntry;
use ssync_core::checksum::{ChecksumAlgorithm, calculate_file_checksum, calculate_reader_checksum};
use ssync_core::delta::{
    MatchStats, SumStruct, generate_basis_signatures, generate_delta, reconstruct_file,
};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Match type for alternate destination directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltDestMatch {
    Link(usize),
    Copy(usize),
    Compare(usize),
}

/// Options for local synchronization execution.
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub dry_run: bool,
    pub verbose: u8,
    pub quiet: bool,
    pub preserve_perms: bool,
    pub preserve_times: bool,
    pub omit_dir_times: bool,
    pub omit_link_times: bool,
    pub executability: bool,
    pub preserve_owner: bool,
    pub preserve_group: bool,
    pub hard_links: bool,
    pub existing: bool,
    pub ignore_existing: bool,
    pub delete: bool,
    pub delete_before: bool,
    pub delete_during: bool,
    pub delete_delay: bool,
    pub delete_after: bool,
    pub delete_excluded: bool,
    pub max_delete: Option<usize>,
    pub remove_source_files: bool,
    pub delay_updates: bool,
    pub backup: bool,
    pub backup_dir: Option<PathBuf>,
    pub temp_dir: Option<PathBuf>,
    pub suffix: String,
    pub itemize_changes: bool,
    pub checksum: bool,
    pub update: bool,
    pub ignore_times: bool,
    pub size_only: bool,
    pub modify_window: i32,
    pub inplace: bool,
    pub append: bool,
    pub append_verify: bool,
    pub force: bool,
    pub block_size: Option<usize>,
    pub no_whole_file: bool,
    pub whole_file: bool,
    pub stats: bool,
    pub debug: Option<String>,
    pub info: Option<String>,
    pub log_file: Option<PathBuf>,
    pub link_dest: Vec<PathBuf>,
    pub copy_dest: Vec<PathBuf>,
    pub compare_dest: Vec<PathBuf>,
    pub dest_is_file: bool,
    pub relative: bool,
    pub sparse: bool,
    pub copy_links: bool,
    pub copy_dirlinks: bool,
    pub keep_dirlinks: bool,
    pub copy_unsafe_links: bool,
    pub mkpath: bool,
    pub filters: FilterSet,
}

pub struct SyncEngine {
    options: SyncOptions,
}

fn format_thousands(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut parts = Vec::new();
    while n > 0 {
        let rem = n % 1000;
        n /= 1000;
        if n > 0 {
            parts.push(format!("{:03}", rem));
        } else {
            parts.push(format!("{}", rem));
        }
    }
    parts.reverse();
    parts.join(",")
}

fn copy_sparse(reader: &mut File, writer: &mut File, total_len: u64) -> std::io::Result<u64> {
    let mut buffer = [0u8; 8192];
    let mut pending_zeros = 0u64;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let chunk = &buffer[..n];
        let is_all_zero = chunk.iter().all(|&b| b == 0);

        if is_all_zero {
            pending_zeros += n as u64;
        } else {
            if pending_zeros > 0 {
                writer.seek(SeekFrom::Current(pending_zeros as i64))?;
                pending_zeros = 0;
            }
            writer.write_all(chunk)?;
        }
    }

    if pending_zeros > 0 {
        writer.seek(SeekFrom::Current(pending_zeros as i64))?;
    }
    writer.set_len(total_len)?;
    Ok(total_len)
}

fn count_missing_dirs(path: &Path) -> usize {
    let mut count = 0;
    let mut curr = path;
    while !curr.as_os_str().is_empty() && !curr.exists() {
        count += 1;
        if let Some(parent) = curr.parent() {
            if parent == curr {
                break;
            }
            curr = parent;
        } else {
            break;
        }
    }
    count
}

impl SyncEngine {
    pub fn new(options: SyncOptions) -> Self {
        Self { options }
    }

    /// Determines if two modification timestamps are considered identical according to `--modify-window`.
    pub fn same_mtime(
        src_sec: i64,
        src_nsec: i64,
        dst_sec: i64,
        dst_nsec: i64,
        modify_window: i32,
    ) -> bool {
        if modify_window == 0 {
            src_sec == dst_sec
        } else if modify_window < 0 {
            src_sec == dst_sec && src_nsec == dst_nsec
        } else {
            (src_sec - dst_sec).abs() <= modify_window as i64
        }
    }

    /// Determines if a destination file is newer than the source file according to `-u, --update`.
    pub fn is_newer(
        dst_sec: i64,
        dst_nsec: i64,
        src_sec: i64,
        src_nsec: i64,
        modify_window: i32,
    ) -> bool {
        if modify_window > 0 {
            dst_sec > src_sec + modify_window as i64
        } else if dst_sec != src_sec {
            dst_sec > src_sec
        } else {
            dst_nsec > src_nsec
        }
    }

    fn find_alt_dest_match(
        &self,
        rel_path: &Path,
        entry_meta: &FileMetadata,
        dest_root: &Path,
    ) -> Option<(AltDestMatch, PathBuf)> {
        for (i, ldir) in self.options.link_dest.iter().enumerate() {
            let full_dir = if ldir.is_absolute() {
                ldir.clone()
            } else {
                crate::path::clean_path(&dest_root.join(ldir))
            };
            let cand = full_dir.join(rel_path);
            if let Ok(meta) = FileMetadata::from_path(&cand, false) {
                if meta.file_type == entry_meta.file_type {
                    if meta.file_type == FileType::Directory {
                        return Some((AltDestMatch::Link(i), cand));
                    }
                    if meta.size == entry_meta.size
                        && Self::same_mtime(
                            entry_meta.mtime_sec,
                            entry_meta.mtime_nsec,
                            meta.mtime_sec,
                            meta.mtime_nsec,
                            self.options.modify_window,
                        )
                    {
                        return Some((AltDestMatch::Link(i), cand));
                    }
                }
            }
        }

        for (i, cdir) in self.options.copy_dest.iter().enumerate() {
            let full_dir = if cdir.is_absolute() {
                cdir.clone()
            } else {
                crate::path::clean_path(&dest_root.join(cdir))
            };
            let cand = full_dir.join(rel_path);
            if let Ok(meta) = FileMetadata::from_path(&cand, false) {
                if meta.file_type == entry_meta.file_type {
                    if meta.file_type == FileType::Directory {
                        return Some((AltDestMatch::Copy(i), cand));
                    }
                    if meta.size == entry_meta.size
                        && Self::same_mtime(
                            entry_meta.mtime_sec,
                            entry_meta.mtime_nsec,
                            meta.mtime_sec,
                            meta.mtime_nsec,
                            self.options.modify_window,
                        )
                    {
                        return Some((AltDestMatch::Copy(i), cand));
                    }
                }
            }
        }

        for (i, cmp_dir) in self.options.compare_dest.iter().enumerate() {
            let full_dir = if cmp_dir.is_absolute() {
                cmp_dir.clone()
            } else {
                crate::path::clean_path(&dest_root.join(cmp_dir))
            };
            let cand = full_dir.join(rel_path);
            if let Ok(meta) = FileMetadata::from_path(&cand, false) {
                if meta.file_type == entry_meta.file_type {
                    if meta.file_type == FileType::Directory {
                        return Some((AltDestMatch::Compare(i), cand));
                    }
                    if meta.size == entry_meta.size
                        && Self::same_mtime(
                            entry_meta.mtime_sec,
                            entry_meta.mtime_nsec,
                            meta.mtime_sec,
                            meta.mtime_nsec,
                            self.options.modify_window,
                        )
                    {
                        return Some((AltDestMatch::Compare(i), cand));
                    }
                }
            }
        }

        None
    }

    fn log_transfer(&self, action: &str, path: &Path) {
        if let Some(ref log_path) = self.options.log_file {
            if let Ok(mut f) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
            {
                use std::io::Write;
                use std::os::unix::ffi::OsStrExt;
                let raw = path.as_os_str().as_bytes();
                let mut escaped = Vec::new();
                for &b in raw {
                    if b < 0x20 || (0x80..=0x9f).contains(&b) || b == 0x7f {
                        escaped.extend_from_slice(format!("\\#{:03o}", b).as_bytes());
                    } else {
                        escaped.push(b);
                    }
                }
                let _ = f.write_all(action.as_bytes());
                let _ = f.write_all(b" ");
                let _ = f.write_all(&escaped);
                let _ = f.write_all(b"\n");
            }
        }
    }

    /// Executes the synchronization of entries into `dest_root`.
    pub fn execute(&self, entries: &[TransferEntry], dest_root: &Path) -> Result<(), FsError> {
        let resolved_root;
        let dest_root = if dest_root.is_dir() && dest_root.is_symlink() {
            if let Ok(canon) = dest_root.canonicalize() {
                resolved_root = canon;
                &resolved_root
            } else {
                dest_root
            }
        } else {
            dest_root
        };
        let dest_existed = dest_root.exists();
        if self.options.dest_is_file {
            let parent = dest_root.parent().unwrap_or_else(|| Path::new(""));
            if !parent.as_os_str().is_empty() && !parent.exists() {
                if !self.options.mkpath {
                    return Err(FsError::ChangeDirFailed(parent.to_path_buf()));
                }
                let missing = count_missing_dirs(parent);
                if (self.options.itemize_changes || self.options.verbose > 0) && !self.options.quiet
                {
                    let word = if missing == 1 {
                        "directory"
                    } else {
                        "directories"
                    };
                    println!("created {} {} for {}", missing, word, parent.display());
                }
                if !self.options.dry_run {
                    fs::create_dir_all(parent)?;
                }
            }
        } else if !dest_existed {
            let parent = dest_root.parent().unwrap_or_else(|| Path::new(""));
            if !parent.as_os_str().is_empty() && !parent.exists() {
                if !self.options.mkpath {
                    return Err(FsError::ChangeDirFailed(dest_root.to_path_buf()));
                }
                let missing = count_missing_dirs(dest_root);
                if (self.options.itemize_changes || self.options.verbose > 0) && !self.options.quiet
                {
                    let word = if missing == 1 {
                        "directory"
                    } else {
                        "directories"
                    };
                    println!("created {} {} for {}", missing, word, dest_root.display());
                }
                if !self.options.dry_run {
                    fs::create_dir_all(dest_root)?;
                }
            } else {
                if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet
                {
                    println!("created directory {}", dest_root.display());
                }
                if !self.options.dry_run {
                    fs::create_dir(dest_root)?;
                }
            }
        }

        let transferred_paths: HashSet<PathBuf> = entries
            .iter()
            .map(|e| e.relative_dest_path.clone())
            .collect();

        let should_delete = self.options.delete
            || self.options.delete_before
            || self.options.delete_during
            || self.options.delete_delay
            || self.options.delete_after
            || self.options.delete_excluded;

        let delete_early = should_delete
            && (self.options.delete_before
                || self.options.delete_during
                || (!self.options.delete_delay && !self.options.delete_after));

        if delete_early {
            self.perform_deletion(dest_root, &transferred_paths)?;
        }

        let mut hardlink_map: HashMap<(u64, u64), (PathBuf, PathBuf)> = HashMap::new();
        let mut dirs_to_update: Vec<(PathBuf, PathBuf, FileMetadata, bool)> = Vec::new();
        let mut delayed_updates: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut delayed_staging_dirs: HashSet<PathBuf> = HashSet::new();
        let mut transferred_names: Vec<String> = Vec::new();
        let mut total_stats = MatchStats::default();
        let mut had_partial_error = false;

        for entry in entries {
            let dest_path = if entry.relative_dest_path.as_os_str().is_empty() {
                dest_root.to_path_buf()
            } else {
                dest_root.join(&entry.relative_dest_path)
            };

            if self.options.existing && !dest_path.exists() {
                continue;
            }

            let res = match entry.metadata.file_type {
                FileType::Directory => self.sync_directory(
                    entry,
                    &dest_path,
                    dest_root,
                    dest_existed,
                    &mut dirs_to_update,
                ),
                FileType::Symlink => self.sync_symlink(entry, &dest_path, dest_root),
                FileType::Regular => self.sync_regular_file(
                    entry,
                    &dest_path,
                    dest_root,
                    &mut hardlink_map,
                    &mut delayed_updates,
                    &mut delayed_staging_dirs,
                    &mut transferred_names,
                    &mut total_stats,
                ),
                _ => self.sync_special(entry, &dest_path, dest_root),
            };

            if let Err(e) = res {
                eprintln!("ssync: {}: {}", dest_path.display(), e);
                had_partial_error = true;
            }
        }

        for name in transferred_names {
            println!("{}", name);
        }

        let delete_late = should_delete && (self.options.delete_delay || self.options.delete_after);
        if delete_late {
            self.perform_deletion(dest_root, &transferred_paths)?;
        }

        // Apply delayed updates if --delay-updates is active
        if !delayed_updates.is_empty() && !self.options.dry_run {
            for (staged_path, final_dest) in &delayed_updates {
                if let Some(parent) = final_dest.parent() {
                    let _ = safe_create_dir_all(dest_root, parent);
                }
                safe_rename_into(dest_root, staged_path, final_dest)?;
            }
            for staging_dir in &delayed_staging_dirs {
                let _ = fs::remove_dir(staging_dir);
            }
        }

        // Apply metadata to directories in post-order (deepest directories first)
        dirs_to_update.sort_by_key(|a| std::cmp::Reverse(a.0.components().count()));
        for (dir_path, _rel_path, meta, _newly_created) in dirs_to_update {
            if !self.options.dry_run {
                meta.apply_to(
                    &dir_path,
                    self.options.preserve_perms,
                    self.options.preserve_times && !self.options.omit_dir_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }
        }

        // Output debug/stats if requested
        if let Some(debug_opt) = &self.options.debug {
            if debug_opt.contains("deltasum") {
                println!(
                    "total: matches={}  hash_hits={}  false_alarms={} data={}",
                    total_stats.matches,
                    total_stats.hash_hits,
                    total_stats.false_alarms,
                    total_stats.literal_bytes
                );
            }
        }

        if self.options.stats {
            println!(
                "Literal data: {} bytes",
                format_thousands(total_stats.literal_bytes)
            );
            println!(
                "Matched data: {} bytes",
                format_thousands(total_stats.matched_bytes)
            );
        }

        if had_partial_error {
            return Err(FsError::PartialTransfer);
        }

        Ok(())
    }

    fn sync_directory(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dest_root: &Path,
        dest_existed: bool,
        dirs_to_update: &mut Vec<(PathBuf, PathBuf, FileMetadata, bool)>,
    ) -> Result<(), FsError> {
        let is_root = entry.relative_dest_path.as_os_str().is_empty();
        use std::os::unix::ffi::OsStrExt;
        let raw_dir = if is_root {
            b"./".to_vec()
        } else if self.options.relative
            && entry.source_path.is_absolute()
            && !entry.source_path.to_string_lossy().contains("/./")
        {
            let mut v = b"/".to_vec();
            v.extend_from_slice(entry.relative_dest_path.as_os_str().as_bytes());
            v.push(b'/');
            v
        } else {
            let mut v = entry.relative_dest_path.as_os_str().as_bytes().to_vec();
            v.push(b'/');
            v
        };
        let display_dir = crate::path::format_terminal_filename(&raw_dir);

        if let Some((_alt_match, _)) =
            self.find_alt_dest_match(&entry.relative_dest_path, &entry.metadata, dest_root)
        {
            if !self.options.dry_run && !dest_path.exists() {
                safe_create_dir_all(dest_root, dest_path)?;
            }
            if self.options.itemize_changes && !self.options.quiet {
                if self.options.verbose > 1 {
                    println!("cd          {}", display_dir);
                }
            } else if self.options.verbose > 1 && !self.options.quiet {
                println!("{} is uptodate", display_dir);
            }
            dirs_to_update.push((
                dest_path.to_path_buf(),
                entry.relative_dest_path.clone(),
                entry.metadata.clone(),
                false,
            ));
            return Ok(());
        }

        let is_dest_symlink = dest_path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if !is_root && is_dest_symlink && !self.options.keep_dirlinks && !self.options.dry_run {
            let _ = fs::remove_file(dest_path);
        }

        let newly_created = if is_root {
            !dest_existed
        } else {
            !dest_path.exists()
        };

        let existing_meta = if !newly_created {
            FileMetadata::from_path(dest_path, false).ok()
        } else {
            None
        };

        if !self.options.dry_run && !dest_path.exists() {
            safe_create_dir_all(dest_root, dest_path)?;
        }

        if self.options.itemize_changes && !self.options.quiet {
            if newly_created {
                println!("cd+++++++++ {}", display_dir);
            } else if let Some(ref emeta) = existing_meta {
                let time_differs = self.options.preserve_times
                    && !self.options.omit_dir_times
                    && !Self::same_mtime(
                        entry.metadata.mtime_sec,
                        entry.metadata.mtime_nsec,
                        emeta.mtime_sec,
                        emeta.mtime_nsec,
                        self.options.modify_window,
                    );
                if time_differs {
                    let t_char = if self.options.preserve_times {
                        't'
                    } else {
                        'T'
                    };
                    println!(".d..{}...... {}", t_char, display_dir);
                } else if self.options.verbose > 1 {
                    println!(".d          {}", display_dir);
                }
            } else if self.options.verbose > 1 {
                println!(".d          {}", display_dir);
            }
        }

        dirs_to_update.push((
            dest_path.to_path_buf(),
            entry.relative_dest_path.clone(),
            entry.metadata.clone(),
            newly_created,
        ));
        Ok(())
    }

    fn sync_symlink(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dest_root: &Path,
    ) -> Result<(), FsError> {
        if self.options.ignore_existing && dest_path.symlink_metadata().is_ok() {
            return Ok(());
        }

        let target = entry
            .metadata
            .symlink_target
            .as_ref()
            .ok_or_else(|| FsError::MetadataError("Symlink target missing".to_string()))?;

        let preserve_link_times = self.options.preserve_times && !self.options.omit_link_times;
        if let Some((alt_type, _cand)) =
            self.find_alt_dest_match(&entry.relative_dest_path, &entry.metadata, dest_root)
        {
            let is_compare = matches!(alt_type, AltDestMatch::Compare(_));
            let is_link = matches!(alt_type, AltDestMatch::Link(_));
            let prefix = if is_compare {
                ".L"
            } else if is_link {
                "hL"
            } else {
                "cL"
            };

            if !is_compare && !self.options.dry_run {
                if let Some(parent) = dest_path.parent() {
                    safe_create_dir_all(dest_root, parent)?;
                }
                if dest_path.symlink_metadata().is_ok() {
                    let _ = fs::remove_file(dest_path);
                }
                std::os::unix::fs::symlink(target, dest_path)?;
                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    preserve_link_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }

            if self.options.itemize_changes && !self.options.quiet {
                if self.options.verbose > 1 {
                    println!(
                        "{}          {} -> {}",
                        prefix,
                        entry.relative_dest_path.display(),
                        target.display()
                    );
                }
            } else if self.options.verbose > 1 && !self.options.quiet {
                println!("{} is uptodate", entry.relative_dest_path.display());
            }
            return Ok(());
        }

        let mut is_new = true;
        let mut target_matches = false;

        if let Ok(existing_target) = fs::read_link(dest_path) {
            is_new = false;
            if existing_target == *target {
                target_matches = true;
            }
        }

        if !target_matches {
            if !self.options.dry_run {
                if let Some(parent) = dest_path.parent() {
                    safe_create_dir_all(dest_root, parent)?;
                }

                if dest_path.symlink_metadata().is_ok() {
                    let _ = fs::remove_file(dest_path);
                }

                std::os::unix::fs::symlink(target, dest_path)?;
                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    preserve_link_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }

            self.log_transfer("cL+++++++++", &entry.relative_dest_path);

            if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet {
                if self.options.itemize_changes {
                    if is_new {
                        println!(
                            "cL+++++++++ {} -> {}",
                            entry.relative_dest_path.display(),
                            target.display()
                        );
                    } else {
                        let time_char = if self.options.preserve_times {
                            't'
                        } else {
                            'T'
                        };
                        println!(
                            "cLc.{}...... {} -> {}",
                            time_char,
                            entry.relative_dest_path.display(),
                            target.display()
                        );
                    }
                } else if self.options.verbose > 1 {
                    println!(
                        "{} -> {}",
                        entry.relative_dest_path.display(),
                        target.display()
                    );
                } else {
                    println!("{}", entry.relative_dest_path.display());
                }
            }
        } else {
            // Target matches: unchanged symlink
            let preserve_link_times = self.options.preserve_times && !self.options.omit_link_times;
            if !self.options.dry_run {
                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    preserve_link_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }
            if self.options.itemize_changes && self.options.verbose > 1 && !self.options.quiet {
                println!(
                    ".L          {} -> {}",
                    entry.relative_dest_path.display(),
                    target.display()
                );
            } else if self.options.verbose > 1 && !self.options.quiet {
                println!("{} is uptodate", entry.relative_dest_path.display());
            }
        }

        if self.options.remove_source_files && !self.options.dry_run {
            let _ = fs::remove_file(&entry.source_path);
        }

        Ok(())
    }

    fn backup_file(
        &self,
        dest_path: &Path,
        rel_path: &Path,
        dest_root: &Path,
        is_delete: bool,
    ) -> Result<(), FsError> {
        if !dest_path.exists() {
            return Ok(());
        }

        let suffix = if self.options.suffix.is_empty() {
            "~"
        } else {
            &self.options.suffix
        };

        let backup_dest = if let Some(ref bdir) = self.options.backup_dir {
            let full_bdir = if bdir.is_absolute() {
                bdir.clone()
            } else {
                dest_root.join(bdir)
            };
            full_bdir.join(rel_path)
        } else {
            PathBuf::from(format!("{}{}", dest_path.display(), suffix))
        };

        if let Some(parent) = backup_dest.parent() {
            let _ = safe_create_dir_all(dest_root, parent);
        }

        if backup_dest.exists() {
            if backup_dest.is_dir() {
                let _ = fs::remove_dir_all(&backup_dest);
            } else {
                let _ = fs::remove_file(&backup_dest);
            }
        }

        let orig_meta = FileMetadata::from_path(dest_path, false).ok();

        if is_delete {
            fs::rename(dest_path, &backup_dest)?;
        } else {
            fs::copy(dest_path, &backup_dest)?;
        }

        if let Some(meta) = orig_meta {
            let _ = meta.apply_to(&backup_dest, true, true, false, false);
        }

        let info_str = self.options.info.as_deref().unwrap_or("");
        if info_str.contains("backup") || self.options.verbose > 0 {
            if self.options.backup_dir.is_some() {
                println!(
                    "backed up {} to {}",
                    rel_path.display(),
                    backup_dest.display()
                );
            } else {
                println!(
                    "backed up {} to {}{}",
                    rel_path.display(),
                    rel_path.display(),
                    suffix
                );
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_regular_file(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dest_root: &Path,
        hardlink_map: &mut HashMap<(u64, u64), (PathBuf, PathBuf)>,
        delayed_updates: &mut Vec<(PathBuf, PathBuf)>,
        delayed_staging_dirs: &mut HashSet<PathBuf>,
        transferred_names: &mut Vec<String>,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        if !self.options.dry_run {
            if let Some(parent) = dest_path.parent() {
                safe_create_dir_all(dest_root, parent)?;
            }
        }

        let raw_display = if entry.relative_dest_path.as_os_str().is_empty() {
            entry
                .source_path
                .file_name()
                .map(|f| f.as_bytes().to_vec())
                .unwrap_or_default()
        } else if self.options.relative
            && entry.source_path.is_absolute()
            && !entry.source_path.to_string_lossy().contains("/./")
        {
            let mut v = b"/".to_vec();
            v.extend_from_slice(entry.relative_dest_path.as_os_str().as_bytes());
            v
        } else {
            entry.relative_dest_path.as_os_str().as_bytes().to_vec()
        };
        let display_path = crate::path::format_terminal_filename(&raw_display);

        // Handle directory replacement when dest_path exists and is a directory
        if let Ok(meta) = fs::symlink_metadata(dest_path) {
            if meta.is_dir() {
                if self.options.force {
                    if !self.options.dry_run {
                        fs::remove_dir_all(dest_path)?;
                    }
                } else {
                    return Err(FsError::Io(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        format!(
                            "cannot overwrite directory with non-directory: {}",
                            dest_path.display()
                        ),
                    )));
                }
            }
        }

        // Check alternate destinations (--copy-dest, --link-dest, --compare-dest)
        if let Some((alt_type, cand)) =
            self.find_alt_dest_match(&entry.relative_dest_path, &entry.metadata, dest_root)
        {
            let is_link = matches!(alt_type, AltDestMatch::Link(_));
            let is_compare = matches!(alt_type, AltDestMatch::Compare(_));
            let prefix = if is_link {
                "hf"
            } else if is_compare {
                ".f"
            } else {
                "cf"
            };

            if self.options.hard_links && entry.metadata.nlink > 1 && !is_compare {
                let key = (entry.metadata.dev, entry.metadata.ino);
                if let Some((first_dest, first_rel)) = hardlink_map.get(&key) {
                    if !is_compare && !self.options.dry_run {
                        if dest_path.symlink_metadata().is_ok() {
                            let _ = fs::remove_file(dest_path);
                        }
                        fs::hard_link(first_dest, dest_path)?;
                    }
                    if (self.options.verbose > 0 || self.options.itemize_changes)
                        && !self.options.quiet
                    {
                        if self.options.itemize_changes {
                            if !is_link || self.options.verbose > 1 {
                                println!("hf          {} => {}", display_path, first_rel.display());
                            }
                        } else if self.options.verbose > 1 {
                            if is_link {
                                println!("{} is uptodate", display_path);
                            } else {
                                transferred_names.push(format!(
                                    "{} => {}",
                                    display_path,
                                    first_rel.display()
                                ));
                            }
                        } else {
                            println!("{}", display_path);
                        }
                    }
                    return Ok(());
                }
            }

            if !is_compare && !self.options.dry_run {
                if let Some(parent) = dest_path.parent() {
                    safe_create_dir_all(dest_root, parent)?;
                }
                if dest_path.symlink_metadata().is_ok() {
                    let _ = fs::remove_file(dest_path);
                }
                if is_link {
                    fs::hard_link(&cand, dest_path)?;
                } else {
                    fs::copy(&cand, dest_path)?;
                }
                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    self.options.preserve_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }

            if self.options.hard_links && entry.metadata.nlink > 1 && !is_compare {
                let key = (entry.metadata.dev, entry.metadata.ino);
                hardlink_map.insert(
                    key,
                    (dest_path.to_path_buf(), entry.relative_dest_path.clone()),
                );
            }

            if self.options.itemize_changes && !self.options.quiet {
                if self.options.verbose > 1 {
                    println!("{}          {}", prefix, display_path);
                }
            } else if self.options.verbose > 1 && !self.options.quiet {
                println!("{} is uptodate", display_path);
            }
            return Ok(());
        }

        // Hard link preservation: if -H is on and nlink > 1
        if self.options.hard_links && entry.metadata.nlink > 1 {
            use std::os::unix::fs::MetadataExt;
            let key = (entry.metadata.dev, entry.metadata.ino);
            if let Some((first_dest, first_rel)) = hardlink_map.get(&key) {
                let already_linked = if let (Ok(meta1), Ok(meta2)) =
                    (fs::metadata(first_dest), fs::metadata(dest_path))
                {
                    meta1.dev() == meta2.dev() && meta1.ino() == meta2.ino()
                } else {
                    false
                };

                if already_linked {
                    if self.options.itemize_changes && !self.options.quiet {
                        if self.options.verbose > 1 {
                            println!("hf          {}", display_path);
                        }
                    } else if self.options.verbose > 1 && !self.options.quiet {
                        println!("{} is uptodate", display_path);
                    }
                    return Ok(());
                }

                if !self.options.dry_run {
                    if dest_path.symlink_metadata().is_ok() {
                        let _ = fs::remove_file(dest_path);
                    }
                    fs::hard_link(first_dest, dest_path)?;
                    if self.options.remove_source_files {
                        let _ = fs::remove_file(&entry.source_path);
                    }
                }
                if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet
                {
                    if self.options.itemize_changes {
                        let time_char = if self.options.preserve_times {
                            't'
                        } else {
                            'T'
                        };
                        println!(
                            "hf..{}...... {} => {}",
                            time_char,
                            display_path,
                            first_rel.display()
                        );
                    } else if self.options.verbose > 1 {
                        transferred_names.push(format!(
                            "{} => {}",
                            display_path,
                            first_rel.display()
                        ));
                    } else {
                        println!("{}", display_path);
                    }
                }
                return Ok(());
            }
        }

        let existing_dest_meta = FileMetadata::from_path(dest_path, false).ok();
        if self.options.ignore_existing && existing_dest_meta.is_some() {
            return Ok(());
        }
        let mut needs_transfer = false;
        let mut itemize_char_content = '.';
        let mut itemize_char_size = '.';
        let mut itemize_char_time = '.';
        let mut itemize_char_perms = '.';

        if let Some(ref dest_meta) = existing_dest_meta {
            if dest_meta.file_type != FileType::Regular {
                needs_transfer = true;
                itemize_char_content = 'c';
            } else if self.options.update
                && Self::is_newer(
                    dest_meta.mtime_sec,
                    dest_meta.mtime_nsec,
                    entry.metadata.mtime_sec,
                    entry.metadata.mtime_nsec,
                    self.options.modify_window,
                )
            {
                let info_str = self.options.info.as_deref().unwrap_or("");
                if info_str.contains("skip") || self.options.verbose > 0 {
                    println!("{} is newer", display_path);
                }
                return Ok(());
            } else if self.options.append || self.options.append_verify {
                if dest_meta.size >= entry.metadata.size {
                    return Ok(());
                }
                needs_transfer = true;
            } else if self.options.size_only {
                if dest_meta.size != entry.metadata.size {
                    needs_transfer = true;
                    itemize_char_size = 's';
                }
            } else if self.options.ignore_times {
                needs_transfer = true;
                itemize_char_content = 'c';
            } else if self.options.checksum {
                if dest_meta.size != entry.metadata.size {
                    needs_transfer = true;
                    itemize_char_size = 's';
                } else {
                    let follow = self.options.copy_links
                        || self.options.copy_unsafe_links
                        || self.options.copy_dirlinks;
                    let src_file =
                        safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
                    let src_sum =
                        calculate_reader_checksum(ChecksumAlgorithm::Md5, src_file, None)?;
                    let dst_sum = calculate_file_checksum(ChecksumAlgorithm::Md5, dest_path, None)?;
                    if src_sum != dst_sum {
                        needs_transfer = true;
                        itemize_char_content = 'c';
                    }
                }
            } else {
                let same_time = Self::same_mtime(
                    entry.metadata.mtime_sec,
                    entry.metadata.mtime_nsec,
                    dest_meta.mtime_sec,
                    dest_meta.mtime_nsec,
                    self.options.modify_window,
                );
                if dest_meta.size != entry.metadata.size {
                    needs_transfer = true;
                    itemize_char_size = 's';
                }
                if !same_time {
                    needs_transfer = true;
                    itemize_char_time = if self.options.preserve_times {
                        't'
                    } else {
                        'T'
                    };
                }
            }

            if self.options.preserve_perms
                && (entry.metadata.mode & 0o7777) != (dest_meta.mode & 0o7777)
            {
                itemize_char_perms = 'p';
            }
        } else {
            needs_transfer = true;
        }

        if needs_transfer {
            if existing_dest_meta.is_some() {
                if self.options.checksum {
                    itemize_char_content = 'c';
                }
                if self.options.preserve_times {
                    itemize_char_time = 't';
                } else {
                    itemize_char_time = 'T';
                }
            }

            if !self.options.dry_run {
                if self.options.backup && existing_dest_meta.is_some() {
                    self.backup_file(dest_path, &entry.relative_dest_path, dest_root, false)?;
                }

                if (self.options.append || self.options.append_verify)
                    && existing_dest_meta.is_some()
                {
                    self.perform_append_transfer(entry, dest_path, total_stats)?;
                } else if self.options.inplace {
                    self.perform_inplace_transfer(entry, dest_path, total_stats)?;
                } else if self.options.delay_updates {
                    self.perform_delay_updates_transfer(
                        entry,
                        dest_path,
                        dest_root,
                        delayed_updates,
                        delayed_staging_dirs,
                        total_stats,
                    )?;
                } else {
                    self.perform_staged_transfer(entry, dest_path, dest_root, total_stats)?;
                }

                if !self.options.delay_updates {
                    if self.options.preserve_perms {
                        entry.metadata.apply_to(
                            dest_path,
                            true,
                            self.options.preserve_times,
                            self.options.preserve_owner,
                            self.options.preserve_group,
                        )?;
                    } else {
                        // Apply default destination perms: source mode & 0o777 & ~umask
                        // SAFETY: umask is a standard POSIX call
                        let umask = unsafe {
                            let u = libc::umask(0);
                            libc::umask(u);
                            u
                        };
                        let new_mode = (entry.metadata.mode & 0o777) & !(umask as u32);
                        let _ =
                            fs::set_permissions(dest_path, fs::Permissions::from_mode(new_mode));
                        if self.options.preserve_times {
                            entry
                                .metadata
                                .apply_to(dest_path, false, true, false, false)?;
                        }
                    }
                }

                self.log_transfer(">f+++++++++", &entry.relative_dest_path);

                if self.options.remove_source_files {
                    let _ = fs::remove_file(&entry.source_path);
                }
            }

            if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet {
                if self.options.itemize_changes {
                    if existing_dest_meta.is_none() {
                        println!(">f+++++++++ {}", display_path);
                    } else {
                        println!(
                            ">f{}{}{}{}..... {}",
                            itemize_char_content,
                            itemize_char_size,
                            itemize_char_time,
                            itemize_char_perms,
                            display_path
                        );
                    }
                } else if self.options.verbose > 1 {
                    transferred_names.push(display_path);
                } else {
                    println!("{}", display_path);
                }
            }
        } else if let Some(ref dest_meta) = existing_dest_meta {
            let mode_differs = self.options.preserve_perms
                && (entry.metadata.mode & 0o7777) != (dest_meta.mode & 0o7777);
            let time_differs = self.options.preserve_times
                && !Self::same_mtime(
                    entry.metadata.mtime_sec,
                    entry.metadata.mtime_nsec,
                    dest_meta.mtime_sec,
                    dest_meta.mtime_nsec,
                    self.options.modify_window,
                );

            if !self.options.dry_run {
                if self.options.preserve_perms {
                    entry.metadata.apply_to(
                        dest_path,
                        true,
                        self.options.preserve_times,
                        self.options.preserve_owner,
                        self.options.preserve_group,
                    )?;
                } else {
                    if self.options.executability {
                        let src_exec = (entry.metadata.mode & 0o111) != 0;
                        let mut new_mode = dest_meta.mode;
                        if !src_exec {
                            new_mode &= !0o111;
                        } else if (dest_meta.mode & 0o111) == 0 {
                            new_mode |= (new_mode & 0o444) >> 2;
                        }
                        if new_mode != dest_meta.mode {
                            let _ = fs::set_permissions(
                                dest_path,
                                fs::Permissions::from_mode(new_mode),
                            );
                        }
                    }
                    if self.options.preserve_times {
                        entry
                            .metadata
                            .apply_to(dest_path, false, true, false, false)?;
                    }
                }
            }

            if self.options.itemize_changes && !self.options.quiet {
                if mode_differs || time_differs {
                    let t_char = if time_differs { 't' } else { '.' };
                    let p_char = if mode_differs { 'p' } else { '.' };
                    println!(".f..{}{}..... {}", t_char, p_char, display_path);
                } else if self.options.verbose > 1 {
                    println!(".f          {}", display_path);
                }
            } else if self.options.verbose > 1 && !self.options.quiet {
                println!("{} is uptodate", display_path);
            }
        }

        if self.options.hard_links && entry.metadata.nlink > 1 {
            let key = (entry.metadata.dev, entry.metadata.ino);
            hardlink_map.insert(
                key,
                (dest_path.to_path_buf(), entry.relative_dest_path.clone()),
            );
        }

        Ok(())
    }

    fn perform_append_transfer(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        let dest_meta = fs::metadata(dest_path)?;
        let dst_len = dest_meta.len();
        let src_len = entry.metadata.size;

        if dst_len >= src_len {
            return Ok(());
        }

        let follow =
            self.options.copy_links || self.options.copy_unsafe_links || self.options.copy_dirlinks;
        let mut src_file =
            safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
        src_file.seek(SeekFrom::Start(dst_len))?;

        let mut dst_file = OpenOptions::new().write(true).open(dest_path)?;
        dst_file.seek(SeekFrom::Start(dst_len))?;

        let appended = std::io::copy(&mut src_file, &mut dst_file)?;
        dst_file.flush()?;
        total_stats.literal_bytes += appended;

        if self.options.append_verify {
            let src_file =
                safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
            let src_sum = calculate_reader_checksum(ChecksumAlgorithm::Md5, src_file, None)?;
            let dst_sum = calculate_file_checksum(ChecksumAlgorithm::Md5, dest_path, None)?;
            if src_sum != dst_sum {
                let mut full_src =
                    safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
                let mut full_dst = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(dest_path)?;
                let full_copied = std::io::copy(&mut full_src, &mut full_dst)?;
                full_dst.flush()?;
                total_stats.literal_bytes += full_copied;
            }
        }

        Ok(())
    }

    fn perform_inplace_transfer(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        let is_delta_mode = self.options.no_whole_file && dest_path.exists();
        let follow =
            self.options.copy_links || self.options.copy_unsafe_links || self.options.copy_dirlinks;

        if is_delta_mode {
            let basis = fs::read(dest_path)?;
            let mut src_file =
                safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
            let mut source = Vec::new();
            src_file.read_to_end(&mut source)?;
            let sums = SumStruct::calculate(
                basis.len() as u64,
                self.options.block_size,
                ChecksumAlgorithm::Md5,
            );
            let sigs = generate_basis_signatures(&basis, &sums, ChecksumAlgorithm::Md5, None);
            let (tokens, stats) =
                generate_delta(&source, &sigs, &sums, ChecksumAlgorithm::Md5, None, true);

            total_stats.hash_hits += stats.hash_hits;
            total_stats.false_alarms += stats.false_alarms;
            total_stats.matches += stats.matches;
            total_stats.literal_bytes += stats.literal_bytes;
            total_stats.matched_bytes += stats.matched_bytes;

            let reconstructed = reconstruct_file(&basis, &tokens, sums.block_len)
                .map_err(|e| FsError::Io(std::io::Error::other(e.to_string())))?;

            let mut f = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(dest_path)?;
            f.write_all(&reconstructed)?;
            f.flush()?;
        } else {
            let mut src_file =
                safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
            let mut dst_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(dest_path)?;
            let copied = if self.options.sparse {
                copy_sparse(&mut src_file, &mut dst_file, entry.metadata.size)?
            } else {
                std::io::copy(&mut src_file, &mut dst_file)?
            };
            dst_file.flush()?;
            total_stats.literal_bytes += copied;
        }

        Ok(())
    }

    fn perform_staged_transfer(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dest_root: &Path,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        let rand_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_path = if let Some(ref tdir) = self.options.temp_dir {
            tdir.join(format!(".ssync-tmp.{}.{}", std::process::id(), rand_id))
        } else {
            let parent = dest_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!(".ssync-tmp.{}.{}", std::process::id(), rand_id))
        };

        let is_delta_mode = self.options.no_whole_file && dest_path.exists();
        let follow =
            self.options.copy_links || self.options.copy_unsafe_links || self.options.copy_dirlinks;

        if is_delta_mode {
            let basis = fs::read(dest_path)?;
            let mut src_file =
                safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)?;
            let mut source = Vec::new();
            src_file.read_to_end(&mut source)?;
            let sums = SumStruct::calculate(
                basis.len() as u64,
                self.options.block_size,
                ChecksumAlgorithm::Md5,
            );
            let sigs = generate_basis_signatures(&basis, &sums, ChecksumAlgorithm::Md5, None);
            let (tokens, stats) =
                generate_delta(&source, &sigs, &sums, ChecksumAlgorithm::Md5, None, false);

            total_stats.hash_hits += stats.hash_hits;
            total_stats.false_alarms += stats.false_alarms;
            total_stats.matches += stats.matches;
            total_stats.literal_bytes += stats.literal_bytes;
            total_stats.matched_bytes += stats.matched_bytes;

            let reconstructed = reconstruct_file(&basis, &tokens, sums.block_len)
                .map_err(|e| FsError::Io(std::io::Error::other(e.to_string())))?;

            if let Err(e) = fs::write(&tmp_path, &reconstructed) {
                let _ = fs::remove_file(&tmp_path);
                return Err(FsError::Io(e));
            }
        } else {
            let copy_res =
                if self.options.sparse {
                    safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)
                        .and_then(|mut src| {
                            OpenOptions::new()
                                .write(true)
                                .create(true)
                                .truncate(true)
                                .open(&tmp_path)
                                .and_then(|mut dst| {
                                    copy_sparse(&mut src, &mut dst, entry.metadata.size)
                                })
                        })
                } else {
                    safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow)
                        .and_then(|mut src| {
                            OpenOptions::new()
                                .write(true)
                                .create(true)
                                .truncate(true)
                                .open(&tmp_path)
                                .and_then(|mut dst| std::io::copy(&mut src, &mut dst))
                        })
                };
            match copy_res {
                Ok(copied) => total_stats.literal_bytes += copied,
                Err(e) => {
                    let _ = fs::remove_file(&tmp_path);
                    return Err(FsError::Io(e));
                }
            }
        }

        if let Err(e) = safe_rename_into(dest_root, &tmp_path, dest_path) {
            if let FsError::Io(ref io_err) = e {
                if io_err.raw_os_error() == Some(libc::EXDEV)
                    || io_err.kind() == std::io::ErrorKind::CrossesDevices
                {
                    if dest_path.exists() {
                        let _ = fs::remove_file(dest_path);
                    }
                    let res = fs::copy(&tmp_path, dest_path);
                    let _ = fs::remove_file(&tmp_path);
                    res?;
                    return Ok(());
                }
            }
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }
        Ok(())
    }

    fn perform_delay_updates_transfer(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dest_root: &Path,
        delayed_updates: &mut Vec<(PathBuf, PathBuf)>,
        delayed_staging_dirs: &mut HashSet<PathBuf>,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        let parent = dest_path.parent().unwrap_or_else(|| Path::new("."));
        let staging_dir = parent.join(".~tmp~");
        let _ = safe_create_dir_all(dest_root, &staging_dir);
        delayed_staging_dirs.insert(staging_dir.clone());

        let file_name = dest_path.file_name().unwrap();
        let staged_file = staging_dir.join(file_name);

        let follow =
            self.options.copy_links || self.options.copy_unsafe_links || self.options.copy_dirlinks;
        let copied = if self.options.sparse {
            safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow).and_then(
                |mut src| {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&staged_file)
                        .and_then(|mut dst| copy_sparse(&mut src, &mut dst, entry.metadata.size))
                },
            )?
        } else {
            safe_open_source(&entry.source_path, entry.source_root.as_deref(), follow).and_then(
                |mut src| {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&staged_file)
                        .and_then(|mut dst| std::io::copy(&mut src, &mut dst))
                },
            )?
        };
        total_stats.literal_bytes += copied;

        delayed_updates.push((staged_file, dest_path.to_path_buf()));
        Ok(())
    }

    fn sync_special(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dest_root: &Path,
    ) -> Result<(), FsError> {
        if !self.options.dry_run {
            if let Some(parent) = dest_path.parent() {
                safe_create_dir_all(dest_root, parent)?;
            }
            if dest_path.symlink_metadata().is_ok() {
                let _ = fs::remove_file(dest_path);
            }
            let c_path = std::ffi::CString::new(dest_path.as_os_str().as_encoded_bytes())
                .map_err(|e| FsError::PathError(e.to_string()))?;
            let mode = entry.metadata.mode as libc::mode_t;
            let dev = entry.metadata.dev as libc::dev_t;
            // SAFETY: c_path is a valid null-terminated C string.
            let res = unsafe { libc::mknod(c_path.as_ptr(), mode, dev) };
            if res != 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::EPERM) {
                    return Err(FsError::Io(err));
                }
            }
        }
        Ok(())
    }

    fn perform_deletion(
        &self,
        dest_root: &Path,
        transferred_paths: &HashSet<PathBuf>,
    ) -> Result<(), FsError> {
        if !dest_root.exists() {
            return Ok(());
        }

        let mut deletion_filters = self.options.filters.clone();
        let mut dest_entries = Vec::new();
        self.collect_dest_entries(
            dest_root,
            Path::new(""),
            &mut deletion_filters,
            &mut dest_entries,
        )?;

        // Sort descending so child files/dirs are deleted before parent dirs
        dest_entries.sort_by_key(|a| std::cmp::Reverse(a.0.components().count()));

        let mut delete_count = 0;

        for (rel_path, is_dir, entry_filters) in dest_entries {
            if transferred_paths.contains(&rel_path) {
                continue;
            }

            let rel_str = rel_path.to_string_lossy();

            if entry_filters.is_protected_from_deletion(&rel_str, is_dir) {
                continue;
            }

            if !entry_filters.is_included_on_receiver(&rel_str, is_dir)
                && !self.options.delete_excluded
            {
                continue;
            }

            if let Some(max_del) = self.options.max_delete {
                if delete_count >= max_del {
                    return Err(FsError::DeleteLimitReached(max_del));
                }
            }

            delete_count += 1;
            let full_path = dest_root.join(&rel_path);

            if self.options.itemize_changes && !self.options.quiet {
                println!("*deleting   {}", rel_path.display());
            } else if (self.options.verbose > 0
                || self.options.delete
                || self.options.delete_excluded)
                && !self.options.quiet
            {
                println!("deleting {}", rel_path.display());
            }

            if !self.options.dry_run {
                let is_already_backup = self.options.backup_dir.is_none()
                    && full_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.ends_with(&self.options.suffix))
                        .unwrap_or(false);

                if self.options.backup && !is_dir && !is_already_backup {
                    self.backup_file(&full_path, &rel_path, dest_root, true)?;
                } else if is_dir {
                    let _ = fs::remove_dir(&full_path);
                } else {
                    let _ = fs::remove_file(&full_path);
                }
            }
        }

        Ok(())
    }

    fn collect_dest_entries(
        &self,
        base: &Path,
        rel: &Path,
        filters: &mut FilterSet,
        out: &mut Vec<(PathBuf, bool, FilterSet)>,
    ) -> Result<(), FsError> {
        let dir = base.join(rel);
        let mut dir_filters = filters.clone();
        let new_rules =
            dir_filters.load_dir_merge_rules(&dir, rel, crate::filter::RuleSide::Receiver);
        if !new_rules.is_empty() {
            dir_filters.prepend_rules(new_rules);
        }

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let child_rel = rel.join(file_name);
            let meta = entry.file_type()?;
            let is_dir = meta.is_dir();
            out.push((child_rel.clone(), is_dir, dir_filters.clone()));
            if is_dir {
                self.collect_dest_entries(base, &child_rel, &mut dir_filters, out)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_mtime_modify_window() {
        assert!(SyncEngine::same_mtime(100, 500, 100, 999, 0));
        assert!(!SyncEngine::same_mtime(100, 0, 101, 0, 0));
        assert!(SyncEngine::same_mtime(100, 500, 100, 500, -1));
        assert!(!SyncEngine::same_mtime(100, 500, 100, 501, -1));
        assert!(SyncEngine::same_mtime(100, 0, 102, 0, 2));
        assert!(SyncEngine::same_mtime(102, 0, 100, 0, 2));
        assert!(!SyncEngine::same_mtime(100, 0, 103, 0, 2));
    }

    #[test]
    fn test_is_newer() {
        assert!(SyncEngine::is_newer(105, 0, 100, 0, 0));
        assert!(!SyncEngine::is_newer(95, 0, 100, 0, 0));
        assert!(SyncEngine::is_newer(100, 200, 100, 100, 0));
        assert!(!SyncEngine::is_newer(100, 100, 100, 200, 0));
        assert!(!SyncEngine::is_newer(101, 0, 100, 0, 2));
        assert!(SyncEngine::is_newer(103, 0, 100, 0, 2));
    }
}
