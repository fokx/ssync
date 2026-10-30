//! Synchronization engine executing local filesystem transfers.

use crate::FsError;
use crate::metadata::{FileMetadata, FileType};
use crate::walker::TransferEntry;
use std::collections::{HashMap, HashSet};
use std::fs;
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
}

pub struct SyncEngine {
    options: SyncOptions,
}

impl SyncEngine {
    pub fn new(options: SyncOptions) -> Self {
        Self { options }
    }

    /// Executes the synchronization of entries into `dest_root`.
    pub fn execute(&self, entries: &[TransferEntry], dest_root: &Path) -> Result<(), FsError> {
        if !self.options.dry_run && !dest_root.exists() {
            fs::create_dir_all(dest_root)?;
        }

        let mut hardlink_map: HashMap<(u64, u64), PathBuf> = HashMap::new();
        let mut dirs_to_update: Vec<(PathBuf, FileMetadata)> = Vec::new();
        let mut transferred_paths: HashSet<PathBuf> = HashSet::new();

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
                    self.sync_regular_file(entry, &dest_path, &mut hardlink_map)?;
                }
                _ => {
                    // Other file types (fifos, etc.)
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

            // Remove existing destination file or symlink if present
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

        if self.options.verbose > 0 && !self.options.quiet {
            if self.options.verbose > 1 {
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
    ) -> Result<(), FsError> {
        if !self.options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
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
                if self.options.verbose > 0 && !self.options.quiet {
                    println!("{}", entry.relative_dest_path.display());
                }
                return Ok(());
            }
        }

        // Check if file is already up to date (quick check: size and mtime)
        let needs_copy = if let Ok(dest_meta) = FileMetadata::from_path(dest_path, false) {
            dest_meta.size != entry.metadata.size
                || dest_meta.mtime_sec != entry.metadata.mtime_sec
                || dest_meta.mtime_nsec != entry.metadata.mtime_nsec
        } else {
            true
        };

        if needs_copy {
            if !self.options.dry_run {
                if dest_path.symlink_metadata().is_ok() {
                    let _ = fs::remove_file(dest_path);
                }
                fs::copy(&entry.source_path, dest_path)?;
                entry.metadata.apply_to(
                    dest_path,
                    self.options.preserve_perms,
                    self.options.preserve_times,
                    self.options.preserve_owner,
                    self.options.preserve_group,
                )?;
            }

            if self.options.verbose > 0 && !self.options.quiet {
                println!("{}", entry.relative_dest_path.display());
            }
        }

        if self.options.hard_links && entry.metadata.nlink > 1 {
            let key = (entry.metadata.dev, entry.metadata.ino);
            hardlink_map.insert(key, dest_path.to_path_buf());
        }

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
