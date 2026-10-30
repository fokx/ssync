//! Filesystem metadata representation, extraction, and application.

use crate::FsError;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The type of filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    Symlink,
    Fifo,
    Socket,
    BlockDevice,
    CharDevice,
    Other,
}

/// Represents file metadata preserved during synchronization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub file_type: FileType,
    pub mode: u32,
    pub size: u64,
    pub mtime_sec: i64,
    pub mtime_nsec: i64,
    pub atime_sec: i64,
    pub atime_nsec: i64,
    pub uid: u32,
    pub gid: u32,
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
    pub symlink_target: Option<PathBuf>,
}

impl FileMetadata {
    /// Reads metadata from `path`. If `dereference` is false, symlinks are not followed.
    pub fn from_path(path: &Path, dereference: bool) -> Result<Self, FsError> {
        let meta = if dereference {
            fs::metadata(path)?
        } else {
            fs::symlink_metadata(path)?
        };

        let raw_mode = meta.mode();
        let file_type = if meta.is_symlink() {
            FileType::Symlink
        } else if meta.is_dir() {
            FileType::Directory
        } else if meta.is_file() {
            FileType::Regular
        } else {
            let mode_type = raw_mode & libc::S_IFMT;
            if mode_type == libc::S_IFIFO {
                FileType::Fifo
            } else if mode_type == libc::S_IFSOCK {
                FileType::Socket
            } else if mode_type == libc::S_IFBLK {
                FileType::BlockDevice
            } else if mode_type == libc::S_IFCHR {
                FileType::CharDevice
            } else {
                FileType::Other
            }
        };

        let symlink_target = if file_type == FileType::Symlink {
            Some(fs::read_link(path)?)
        } else {
            None
        };

        Ok(Self {
            file_type,
            mode: raw_mode & 0o7777,
            size: meta.len(),
            mtime_sec: meta.mtime(),
            mtime_nsec: meta.mtime_nsec(),
            atime_sec: meta.atime(),
            atime_nsec: meta.atime_nsec(),
            uid: meta.uid(),
            gid: meta.gid(),
            dev: meta.dev(),
            ino: meta.ino(),
            nlink: meta.nlink(),
            symlink_target,
        })
    }

    /// Applies preserved metadata (mode, times, ownership) to the target path.
    pub fn apply_to(
        &self,
        path: &Path,
        preserve_perms: bool,
        preserve_times: bool,
        preserve_owner: bool,
        preserve_group: bool,
    ) -> Result<(), FsError> {
        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|e| FsError::PathError(e.to_string()))?;

        // 1. Ownership (lchown)
        if preserve_owner || preserve_group {
            let uid = if preserve_owner {
                self.uid
            } else {
                u32::MAX // -1 in unsigned, leaves unchanged
            };
            let gid = if preserve_group { self.gid } else { u32::MAX };

            // SAFETY: c_path is a valid null-terminated C string.
            let res = unsafe { libc::lchown(c_path.as_ptr(), uid, gid) };
            if res != 0 {
                let err = std::io::Error::last_os_error();
                // Ignore EPERM when not running as root
                if err.raw_os_error() != Some(libc::EPERM) {
                    return Err(FsError::Io(err));
                }
            }
        }

        // 2. Permissions (chmod). On Linux, chmod on symlinks is not supported.
        if preserve_perms && self.file_type != FileType::Symlink {
            fs::set_permissions(path, fs::Permissions::from_mode(self.mode))?;
        }

        // 3. Timestamps (utimensat)
        if preserve_times {
            let times = [
                libc::timespec {
                    tv_sec: self.atime_sec,
                    tv_nsec: self.atime_nsec,
                },
                libc::timespec {
                    tv_sec: self.mtime_sec,
                    tv_nsec: self.mtime_nsec,
                },
            ];

            // SAFETY: c_path is a valid null-terminated string, and times is a 2-element timespec array.
            let res = unsafe {
                libc::utimensat(
                    libc::AT_FDCWD,
                    c_path.as_ptr(),
                    times.as_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if res != 0 {
                return Err(FsError::Io(std::io::Error::last_os_error()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_file_metadata_roundtrip() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("testfile");
        fs::write(&file_path, b"hello world").unwrap();

        let meta = FileMetadata::from_path(&file_path, false).unwrap();
        assert_eq!(meta.file_type, FileType::Regular);
        assert_eq!(meta.size, 11);

        let target_path = dir.path().join("targetfile");
        fs::write(&target_path, b"hello world").unwrap();

        meta.apply_to(&target_path, true, true, false, false)
            .unwrap();
        let target_meta = FileMetadata::from_path(&target_path, false).unwrap();
        assert_eq!(target_meta.mode, meta.mode);
        assert_eq!(target_meta.mtime_sec, meta.mtime_sec);
        assert_eq!(target_meta.mtime_nsec, meta.mtime_nsec);
    }

    #[test]
    fn test_symlink_metadata() {
        let dir = tempdir().unwrap();
        let link_path = dir.path().join("sym");
        std::os::unix::fs::symlink("referent", &link_path).unwrap();

        let meta = FileMetadata::from_path(&link_path, false).unwrap();
        assert_eq!(meta.file_type, FileType::Symlink);
        assert_eq!(meta.symlink_target, Some(PathBuf::from("referent")));
    }
}
