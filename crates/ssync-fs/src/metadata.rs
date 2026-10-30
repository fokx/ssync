//! Filesystem metadata representation and conversion.

use std::time::SystemTime;

/// Represents file metadata preserved during synchronization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub mode: u32,
    pub size: u64,
    pub mtime: SystemTime,
    pub uid: u32,
    pub gid: u32,
    pub is_symlink: bool,
    pub is_dir: bool,
}
