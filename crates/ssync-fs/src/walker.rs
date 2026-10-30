//! Directory scanner and transfer entry collector.

use crate::FsError;
use crate::filter::FilterSet;
use crate::metadata::{FileMetadata, FileType};
use crate::path::SourcePath;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Options controlling traversal.
#[derive(Debug, Clone, Default)]
pub struct WalkOptions {
    pub recursive: bool,
    pub dirs: bool,
    pub links: bool,
    pub copy_links: bool,
    pub copy_dirlinks: bool,
    pub copy_unsafe_links: bool,
    pub safe_links: bool,
    pub relative: bool,
    pub no_implied_dirs: bool,
    pub filters: FilterSet,
    pub dest_is_file: bool,
    pub max_size: Option<u64>,
    pub min_size: Option<u64>,
    pub prune_empty_dirs: bool,
}

/// Represents an item scheduled for transfer.
#[derive(Debug, Clone)]
pub struct TransferEntry {
    pub source_path: PathBuf,
    pub relative_dest_path: PathBuf,
    pub metadata: FileMetadata,
    pub is_implied_dir: bool,
}

pub struct Walker {
    options: WalkOptions,
}

impl Walker {
    pub fn new(options: WalkOptions) -> Self {
        Self { options }
    }

    /// Scans a list of source paths and generates a list of transfer entries.
    pub fn scan_sources(&self, sources: &[SourcePath]) -> Result<Vec<TransferEntry>, FsError> {
        let mut entries = Vec::new();
        let mut seen_dest_paths = HashSet::new();

        for source in sources {
            self.scan_single_source(source, &mut entries, &mut seen_dest_paths)?;
        }

        if self.options.prune_empty_dirs {
            // Collect set of all relative paths for non-directory items
            let non_dirs: Vec<PathBuf> = entries
                .iter()
                .filter(|e| e.metadata.file_type != FileType::Directory)
                .map(|e| e.relative_dest_path.clone())
                .collect();

            entries.retain(|e| {
                if e.metadata.file_type != FileType::Directory {
                    return true;
                }
                if e.relative_dest_path.as_os_str().is_empty() {
                    return true; // Keep transfer root
                }
                non_dirs
                    .iter()
                    .any(|nd| nd.starts_with(&e.relative_dest_path))
            });
        }

        Ok(entries)
    }

    fn scan_single_source(
        &self,
        source: &SourcePath,
        entries: &mut Vec<TransferEntry>,
        seen_dest_paths: &mut HashSet<PathBuf>,
    ) -> Result<(), FsError> {
        let mut filters = self.options.filters.clone();

        if self.options.relative {
            // Check if there is an explicit anchor (starts with "./" or contains "/./")
            if let Some(anc) = source.anchor_path() {
                if seen_dest_paths.insert(PathBuf::from("")) {
                    if let Ok(meta) = self.read_meta(&anc) {
                        entries.push(TransferEntry {
                            source_path: anc,
                            relative_dest_path: PathBuf::from(""),
                            metadata: meta,
                            is_implied_dir: false,
                        });
                    }
                }
            }

            // 1. Handle implied parent directories for -R
            let ancestors = source.implied_ancestors();
            for anc in ancestors {
                if seen_dest_paths.insert(anc.clone()) {
                    let meta = if self.options.no_implied_dirs {
                        // Default attributes for implied dirs (0o755)
                        FileMetadata {
                            file_type: FileType::Directory,
                            mode: 0o755,
                            size: 0,
                            mtime_sec: 0,
                            mtime_nsec: 0,
                            atime_sec: 0,
                            atime_nsec: 0,
                            uid: 0,
                            gid: 0,
                            dev: 0,
                            ino: 0,
                            nlink: 1,
                            symlink_target: None,
                        }
                    } else {
                        // Inherit from source ancestor if it exists, else default
                        if anc.exists() {
                            FileMetadata::from_path(&anc, false)?
                        } else {
                            FileMetadata {
                                file_type: FileType::Directory,
                                mode: 0o755,
                                size: 0,
                                mtime_sec: 0,
                                mtime_nsec: 0,
                                atime_sec: 0,
                                atime_nsec: 0,
                                uid: 0,
                                gid: 0,
                                dev: 0,
                                ino: 0,
                                nlink: 1,
                                symlink_target: None,
                            }
                        }
                    };

                    entries.push(TransferEntry {
                        source_path: anc.clone(),
                        relative_dest_path: anc,
                        metadata: meta,
                        is_implied_dir: true,
                    });
                }
            }

            // 2. Scan the source item itself under its relative cut path
            let rel_base = source.relative_cut_path();
            self.scan_path(
                &source.path,
                &rel_base,
                &mut filters,
                entries,
                seen_dest_paths,
            )?;
        } else {
            // Non-relative traversal
            let meta = self.read_meta(&source.path)?;
            if meta.file_type == FileType::Directory {
                if source.has_trailing_slash {
                    // Contents transferred directly under destination root.
                    // Also record the root directory itself to synchronize its attributes to dest_root!
                    if seen_dest_paths.insert(PathBuf::from("")) {
                        entries.push(TransferEntry {
                            source_path: source.path.clone(),
                            relative_dest_path: PathBuf::from(""),
                            metadata: meta,
                            is_implied_dir: false,
                        });
                    }
                    self.scan_dir_contents(
                        &source.path,
                        Path::new(""),
                        &filters,
                        entries,
                        seen_dest_paths,
                    )?;
                } else {
                    // The directory itself is transferred as a subfolder
                    let dir_name = source
                        .path
                        .file_name()
                        .ok_or_else(|| FsError::PathError("Invalid directory name".to_string()))?;
                    let rel_dest = PathBuf::from(dir_name);
                    self.scan_path(
                        &source.path,
                        &rel_dest,
                        &mut filters,
                        entries,
                        seen_dest_paths,
                    )?;
                }
            } else {
                // Single file/symlink transfer
                let rel_dest = if self.options.dest_is_file {
                    PathBuf::from("")
                } else {
                    let file_name = source
                        .path
                        .file_name()
                        .ok_or_else(|| FsError::PathError("Invalid file name".to_string()))?;
                    PathBuf::from(file_name)
                };
                self.scan_path(
                    &source.path,
                    &rel_dest,
                    &mut filters,
                    entries,
                    seen_dest_paths,
                )?;
            }
        }

        Ok(())
    }

    fn read_meta(&self, path: &Path) -> Result<FileMetadata, FsError> {
        if self.options.copy_links {
            FileMetadata::from_path(path, true)
        } else if self.options.copy_dirlinks {
            let direct = FileMetadata::from_path(path, false)?;
            if direct.file_type == FileType::Symlink {
                if let Ok(target_meta) = FileMetadata::from_path(path, true) {
                    if target_meta.file_type == FileType::Directory {
                        return Ok(target_meta);
                    }
                }
            }
            Ok(direct)
        } else {
            FileMetadata::from_path(path, false)
        }
    }

    fn is_unsafe_symlink(rel_dest: &Path, target: &Path) -> bool {
        let target_str = target.to_string_lossy();
        if target_str.is_empty() || target_str.starts_with('/') {
            return true;
        }

        // Strip leading ../ components
        let mut rest = target_str.as_ref();
        let mut leading_dotdots = 0;
        while let Some(stripped) = rest.strip_prefix("../") {
            leading_dotdots += 1;
            rest = stripped.trim_start_matches('/');
        }

        // Reject destinations containing /../ or ending in /.. or equaling ..
        if rest.contains("/../") || rest.ends_with("/..") || rest == ".." {
            return true;
        }

        // Calculate depth of symlink's parent directory from transfer root
        let depth = rel_dest
            .parent()
            .map(|p| p.components().count())
            .unwrap_or(0);

        leading_dotdots > depth
    }

    fn scan_path(
        &self,
        src: &Path,
        rel_dest: &Path,
        current_filters: &mut FilterSet,
        entries: &mut Vec<TransferEntry>,
        seen_dest_paths: &mut HashSet<PathBuf>,
    ) -> Result<(), FsError> {
        let mut meta = self.read_meta(src)?;

        if meta.file_type == FileType::Symlink {
            if let Some(target) = &meta.symlink_target {
                if Self::is_unsafe_symlink(rel_dest, target) {
                    if self.options.safe_links {
                        println!(
                            "ignoring unsafe symlink \"{}\" -> \"{}\"",
                            rel_dest.display(),
                            target.display()
                        );
                        return Ok(());
                    }
                    if self.options.copy_unsafe_links {
                        if let Ok(deref) = FileMetadata::from_path(src, true) {
                            meta = deref;
                        }
                    }
                }
            }
        }

        let is_dir = meta.file_type == FileType::Directory;

        // Filter evaluation
        let rel_str = rel_dest.to_string_lossy();
        if !current_filters.is_included(&rel_str, is_dir) {
            return Ok(());
        }

        // Symlink filter check
        if meta.file_type == FileType::Symlink && !self.options.links && !self.options.copy_links {
            return Ok(());
        }

        // Size filter check for regular files
        if meta.file_type == FileType::Regular {
            if let Some(max_s) = self.options.max_size {
                if meta.size > max_s {
                    return Ok(());
                }
            }
            if let Some(min_s) = self.options.min_size {
                if meta.size < min_s {
                    return Ok(());
                }
            }
        }

        if is_dir {
            if !self.options.recursive && !self.options.dirs {
                // Skipping directory without -r or -d
                return Ok(());
            }

            if seen_dest_paths.insert(rel_dest.to_path_buf()) {
                entries.push(TransferEntry {
                    source_path: src.to_path_buf(),
                    relative_dest_path: rel_dest.to_path_buf(),
                    metadata: meta,
                    is_implied_dir: false,
                });
            }

            if self.options.recursive {
                self.scan_dir_contents(src, rel_dest, current_filters, entries, seen_dest_paths)?;
            }
        } else if seen_dest_paths.insert(rel_dest.to_path_buf()) {
            entries.push(TransferEntry {
                source_path: src.to_path_buf(),
                relative_dest_path: rel_dest.to_path_buf(),
                metadata: meta,
                is_implied_dir: false,
            });
        }

        Ok(())
    }

    fn scan_dir_contents(
        &self,
        dir_path: &Path,
        rel_prefix: &Path,
        parent_filters: &FilterSet,
        entries: &mut Vec<TransferEntry>,
        seen_dest_paths: &mut HashSet<PathBuf>,
    ) -> Result<(), FsError> {
        let mut dir_filters = parent_filters.clone();
        let new_rules =
            dir_filters.load_dir_merge_rules(dir_path, rel_prefix, crate::filter::RuleSide::Sender);
        if !new_rules.is_empty() {
            dir_filters.prepend_rules(new_rules);
        }

        let mut read_entries = Vec::new();
        for entry in fs::read_dir(dir_path)? {
            read_entries.push(entry?.path());
        }
        // Sort lexicographically for deterministic order
        read_entries.sort();

        for child_path in read_entries {
            let child_name = child_path.file_name().unwrap();
            let child_rel_dest = rel_prefix.join(child_name);
            self.scan_path(
                &child_path,
                &child_rel_dest,
                &mut dir_filters,
                entries,
                seen_dest_paths,
            )?;
        }

        Ok(())
    }
}
