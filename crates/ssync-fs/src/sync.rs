//! Synchronization engine executing local filesystem transfers.

use crate::FsError;
use crate::metadata::{FileMetadata, FileType};
use crate::walker::TransferEntry;
use ssync_core::checksum::{ChecksumAlgorithm, calculate_file_checksum};
use ssync_core::delta::{
    MatchStats, SumStruct, generate_basis_signatures, generate_delta, reconstruct_file,
};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Options for local synchronization execution.
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub dry_run: bool,
    pub verbose: u8,
    pub quiet: bool,
    pub preserve_perms: bool,
    pub preserve_times: bool,
    pub preserve_owner: bool,
    pub preserve_group: bool,
    pub hard_links: bool,
    pub existing: bool,
    pub delete: bool,
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
    pub dest_is_file: bool,
}

pub struct SyncEngine {
    options: SyncOptions,
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

    /// Executes the synchronization of entries into `dest_root`.
    pub fn execute(&self, entries: &[TransferEntry], dest_root: &Path) -> Result<(), FsError> {
        if !self.options.dry_run && !dest_root.exists() {
            if self.options.dest_is_file {
                if let Some(parent) = dest_root.parent() {
                    fs::create_dir_all(parent)?;
                }
            } else {
                fs::create_dir_all(dest_root)?;
            }
        }

        let mut hardlink_map: HashMap<(u64, u64), PathBuf> = HashMap::new();
        let mut dirs_to_update: Vec<(PathBuf, FileMetadata)> = Vec::new();
        let mut transferred_paths: HashSet<PathBuf> = HashSet::new();
        let mut total_stats = MatchStats::default();

        for entry in entries {
            let dest_path = if entry.relative_dest_path.as_os_str().is_empty() {
                dest_root.to_path_buf()
            } else {
                dest_root.join(&entry.relative_dest_path)
            };

            transferred_paths.insert(entry.relative_dest_path.clone());

            if self.options.existing && !dest_path.exists() {
                continue;
            }

            match entry.metadata.file_type {
                FileType::Directory => {
                    self.sync_directory(entry, &dest_path, &mut dirs_to_update)?;
                }
                FileType::Symlink => {
                    self.sync_symlink(entry, &dest_path)?;
                }
                FileType::Regular => {
                    self.sync_regular_file(entry, &dest_path, &mut hardlink_map, &mut total_stats)?;
                }
                _ => {
                    self.sync_special(entry, &dest_path)?;
                }
            }
        }

        // Handle deletion of extraneous files if --delete is requested
        if self.options.delete {
            self.perform_deletion(dest_root, &transferred_paths)?;
        }

        // Apply metadata to directories in post-order (deepest directories first)
        dirs_to_update.sort_by_key(|a| std::cmp::Reverse(a.0.components().count()));
        for (dir_path, meta) in dirs_to_update {
            if !self.options.dry_run {
                meta.apply_to(
                    &dir_path,
                    self.options.preserve_perms,
                    self.options.preserve_times,
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
            println!("Literal data: {} bytes", total_stats.literal_bytes);
            println!("Matched data: {} bytes", total_stats.matched_bytes);
        }

        Ok(())
    }

    fn sync_directory(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        dirs_to_update: &mut Vec<(PathBuf, FileMetadata)>,
    ) -> Result<(), FsError> {
        if !self.options.dry_run && !dest_path.exists() {
            fs::create_dir_all(dest_path)?;
        }

        dirs_to_update.push((dest_path.to_path_buf(), entry.metadata.clone()));
        Ok(())
    }

    fn sync_symlink(&self, entry: &TransferEntry, dest_path: &Path) -> Result<(), FsError> {
        let target = entry
            .metadata
            .symlink_target
            .as_ref()
            .ok_or_else(|| FsError::MetadataError("Symlink target missing".to_string()))?;

        if !self.options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            if dest_path.symlink_metadata().is_ok() {
                let _ = fs::remove_file(dest_path);
            }

            std::os::unix::fs::symlink(target, dest_path)?;
            entry.metadata.apply_to(
                dest_path,
                self.options.preserve_perms,
                self.options.preserve_times,
                self.options.preserve_owner,
                self.options.preserve_group,
            )?;
        }

        if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet {
            if self.options.itemize_changes {
                println!("cL+++++++++ {}", entry.relative_dest_path.display());
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

        Ok(())
    }

    fn sync_regular_file(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        hardlink_map: &mut HashMap<(u64, u64), PathBuf>,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        if !self.options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
        }

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

        // Hard link preservation: if -H is on and nlink > 1
        if self.options.hard_links && entry.metadata.nlink > 1 {
            let key = (entry.metadata.dev, entry.metadata.ino);
            if let Some(first_dest) = hardlink_map.get(&key) {
                if !self.options.dry_run {
                    if dest_path.symlink_metadata().is_ok() {
                        let _ = fs::remove_file(dest_path);
                    }
                    fs::hard_link(first_dest, dest_path)?;
                }
                if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet
                {
                    println!("{}", entry.relative_dest_path.display());
                }
                return Ok(());
            }
        }

        let existing_dest_meta = FileMetadata::from_path(dest_path, false).ok();
        let mut needs_transfer = false;
        let mut itemize_char_content = '.';
        let mut itemize_char_size = '.';
        let mut itemize_char_time = '.';

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
                // Destination is newer, skip under -u
                return Ok(());
            } else if self.options.append || self.options.append_verify {
                if dest_meta.size >= entry.metadata.size {
                    // File is already equal or larger, append does not touch it
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
                    let src_sum =
                        calculate_file_checksum(ChecksumAlgorithm::Md5, &entry.source_path, None)?;
                    let dst_sum = calculate_file_checksum(ChecksumAlgorithm::Md5, dest_path, None)?;
                    if src_sum != dst_sum {
                        needs_transfer = true;
                        itemize_char_content = 'c';
                    }
                }
            } else {
                // Default quick check: size and mtime
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
                    itemize_char_time = 't';
                }
            }
        } else {
            // Destination does not exist
            needs_transfer = true;
        }

        if needs_transfer {
            if !self.options.dry_run {
                if (self.options.append || self.options.append_verify)
                    && existing_dest_meta.is_some()
                {
                    self.perform_append_transfer(entry, dest_path, total_stats)?;
                } else if self.options.inplace {
                    self.perform_inplace_transfer(entry, dest_path, total_stats)?;
                } else {
                    self.perform_staged_transfer(entry, dest_path, total_stats)?;
                }

                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    self.options.preserve_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }

            let display_path = if entry.relative_dest_path.as_os_str().is_empty() {
                entry
                    .source_path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                entry.relative_dest_path.display().to_string()
            };

            if (self.options.verbose > 0 || self.options.itemize_changes) && !self.options.quiet {
                if self.options.itemize_changes {
                    if existing_dest_meta.is_none() {
                        println!(">f+++++++++ {}", display_path);
                    } else {
                        println!(
                            ">f{}{}{}...... {}",
                            itemize_char_content,
                            itemize_char_size,
                            itemize_char_time,
                            display_path
                        );
                    }
                } else {
                    println!("{}", display_path);
                }
            }
        } else if let Some(ref dest_meta) = existing_dest_meta {
            // Content unchanged, but attributes may need retouching
            if !self.options.dry_run {
                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    self.options.preserve_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }
            let _ = dest_meta;
        }

        if self.options.hard_links && entry.metadata.nlink > 1 {
            let key = (entry.metadata.dev, entry.metadata.ino);
            hardlink_map.insert(key, dest_path.to_path_buf());
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

        let mut src_file = File::open(&entry.source_path)?;
        src_file.seek(SeekFrom::Start(dst_len))?;

        let mut dst_file = OpenOptions::new().write(true).open(dest_path)?;
        dst_file.seek(SeekFrom::Start(dst_len))?;

        let appended = std::io::copy(&mut src_file, &mut dst_file)?;
        dst_file.flush()?;
        total_stats.literal_bytes += appended;

        if self.options.append_verify {
            let src_sum =
                calculate_file_checksum(ChecksumAlgorithm::Md5, &entry.source_path, None)?;
            let dst_sum = calculate_file_checksum(ChecksumAlgorithm::Md5, dest_path, None)?;
            if src_sum != dst_sum {
                // Prefix mismatch detected by --append-verify: rewrite entire file
                let mut full_src = File::open(&entry.source_path)?;
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

        if is_delta_mode {
            let basis = fs::read(dest_path)?;
            let source = fs::read(&entry.source_path)?;
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
            let mut src_file = File::open(&entry.source_path)?;
            let mut dst_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(dest_path)?;
            let copied = std::io::copy(&mut src_file, &mut dst_file)?;
            dst_file.flush()?;
            total_stats.literal_bytes += copied;
        }

        Ok(())
    }

    fn perform_staged_transfer(
        &self,
        entry: &TransferEntry,
        dest_path: &Path,
        total_stats: &mut MatchStats,
    ) -> Result<(), FsError> {
        let parent = dest_path.parent().unwrap_or_else(|| Path::new("."));
        let rand_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_path = parent.join(format!(".ssync-tmp.{}.{}", std::process::id(), rand_id));

        let is_delta_mode = self.options.no_whole_file && dest_path.exists();

        if is_delta_mode {
            let basis = fs::read(dest_path)?;
            let source = fs::read(&entry.source_path)?;
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

            fs::write(&tmp_path, &reconstructed)?;
        } else {
            let copied = fs::copy(&entry.source_path, &tmp_path)?;
            total_stats.literal_bytes += copied;
        }

        fs::rename(&tmp_path, dest_path)?;
        Ok(())
    }

    fn sync_special(&self, entry: &TransferEntry, dest_path: &Path) -> Result<(), FsError> {
        if !self.options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
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

        let mut dest_entries = Vec::new();
        self.collect_dest_entries(dest_root, Path::new(""), &mut dest_entries)?;

        // Sort descending so child files/dirs are deleted before parent dirs
        dest_entries.sort_by_key(|a| std::cmp::Reverse(a.0.components().count()));

        for (rel_path, is_dir) in dest_entries {
            if !transferred_paths.contains(&rel_path) {
                let full_path = dest_root.join(&rel_path);
                println!("deleting {}", rel_path.display());
                if !self.options.dry_run {
                    if is_dir {
                        let _ = fs::remove_dir(&full_path);
                    } else {
                        let _ = fs::remove_file(&full_path);
                    }
                }
            }
        }

        Ok(())
    }

    fn collect_dest_entries(
        &self,
        base: &Path,
        rel: &Path,
        out: &mut Vec<(PathBuf, bool)>,
    ) -> Result<(), FsError> {
        let dir = base.join(rel);
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let child_rel = rel.join(file_name);
            let meta = entry.file_type()?;
            let is_dir = meta.is_dir();
            out.push((child_rel.clone(), is_dir));
            if is_dir {
                self.collect_dest_entries(base, &child_rel, out)?;
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
        // modify_window == 0: compares integer seconds only
        assert!(SyncEngine::same_mtime(100, 500, 100, 999, 0));
        assert!(!SyncEngine::same_mtime(100, 0, 101, 0, 0));

        // modify_window < 0: exact nanoseconds
        assert!(SyncEngine::same_mtime(100, 500, 100, 500, -1));
        assert!(!SyncEngine::same_mtime(100, 500, 100, 501, -1));

        // modify_window > 0: difference within window
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

        // With modify window
        assert!(!SyncEngine::is_newer(101, 0, 100, 0, 2));
        assert!(SyncEngine::is_newer(103, 0, 100, 0, 2));
    }
}
