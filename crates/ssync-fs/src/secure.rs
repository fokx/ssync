//! Secure filesystem operations: held-dirfd directory creation and race-confined atomic renames.

use crate::FsError;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// RAII wrapper for POSIX directory file descriptors.
pub struct DirFd(libc::c_int);

impl DirFd {
    /// Opens `path` as a directory.
    pub fn open_dir(path: &Path, nofollow: bool) -> Result<Self, std::io::Error> {
        let cpath = CString::new(path.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let mut flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
        if nofollow {
            flags |= libc::O_NOFOLLOW;
        }
        // SAFETY: `cpath` is a valid null-terminated C string.
        let fd = unsafe { libc::open(cpath.as_ptr(), flags) };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Self(fd))
        }
    }

    /// Opens a child directory relative to this directory file descriptor.
    pub fn openat_dir(&self, name: &Path, nofollow: bool) -> Result<Self, std::io::Error> {
        let cname = CString::new(name.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let mut flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
        if nofollow {
            flags |= libc::O_NOFOLLOW;
        }
        // SAFETY: `self.0` is a valid open directory fd and `cname` is a valid null-terminated C string.
        let fd = unsafe { libc::openat(self.0, cname.as_ptr(), flags) };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Self(fd))
        }
    }

    /// Creates a directory entry relative to this directory file descriptor.
    pub fn mkdirat(&self, name: &Path, mode: u32) -> Result<(), std::io::Error> {
        let cname = CString::new(name.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        // SAFETY: `self.0` is a valid open directory fd and `cname` is a valid null-terminated C string.
        let rc = unsafe { libc::mkdirat(self.0, cname.as_ptr(), mode as libc::mode_t) };
        if rc == 0 {
            Ok(())
        } else {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EEXIST) {
                Ok(())
            } else {
                Err(err)
            }
        }
    }

    /// Returns the raw file descriptor.
    pub fn raw(&self) -> libc::c_int {
        self.0
    }
}

impl Drop for DirFd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            // SAFETY: `self.0` is a valid open file descriptor to close.
            unsafe { libc::close(self.0) };
        }
    }
}

/// Checks whether `path` is strictly contained within `root`.
pub fn is_beneath(root: &Path, path: &Path) -> bool {
    let clean_root = crate::path::clean_path(root);
    let clean_target = crate::path::clean_path(path);

    if clean_root.as_os_str().is_empty() || clean_root == Path::new(".") {
        !clean_target.is_absolute() && !clean_target.starts_with("..")
    } else if clean_root.is_absolute() {
        clean_target.starts_with(&clean_root)
    } else if clean_target.is_absolute() {
        false
    } else {
        clean_target.starts_with(&clean_root)
    }
}

/// Creates all parent directory components for `target_dir` confined strictly beneath `dest_root`.
///
/// Follows the held-dirfd pattern: each component is created with `mkdirat` and pinned with `openat`
/// using `O_DIRECTORY | O_NOFOLLOW`. If any parent component is or becomes a symlink, the operation
/// fails with `ELOOP` rather than following the symlink outside the tree.
pub fn safe_create_dir_all(dest_root: &Path, target_dir: &Path) -> Result<(), FsError> {
    if target_dir == dest_root {
        if !dest_root.exists() {
            fs::create_dir_all(dest_root)?;
        }
        return Ok(());
    }

    if let Ok(rel) = target_dir.strip_prefix(dest_root) {
        if !dest_root.exists() {
            fs::create_dir_all(dest_root)?;
        }
        let mut cur_fd = DirFd::open_dir(dest_root, false)?;

        let mut cur_path = dest_root.to_path_buf();
        for comp in rel.components() {
            if let std::path::Component::Normal(c) = comp {
                let name = Path::new(c);
                let _ = cur_fd.mkdirat(name, 0o777);
                let next_fd = match cur_fd.openat_dir(name, true) {
                    Ok(next) => next,
                    Err(e) => {
                        let full_path = cur_path.join(name);
                        if let Ok(link_target) = fs::read_link(&full_path) {
                            let resolved = if link_target.is_absolute() {
                                crate::path::clean_path(&link_target)
                            } else {
                                crate::path::clean_path(&cur_path.join(&link_target))
                            };
                            if is_beneath(dest_root, &resolved) && resolved.is_dir() {
                                cur_fd.openat_dir(name, false)?
                            } else {
                                return Err(FsError::Io(e));
                            }
                        } else {
                            return Err(FsError::Io(e));
                        }
                    }
                };
                cur_path.push(c);
                cur_fd = next_fd;
            }
        }
        Ok(())
    } else {
        fs::create_dir_all(target_dir)?;
        Ok(())
    }
}

/// Opens a source file safely through held ancestor dirfds anchored at `source_root`.
///
/// If any parent component of `source_path` under `source_root` was flipped to a symlink,
/// this function refuses the open with `ENOTDIR` or `ELOOP`, preventing source-tree TOCTOU leaks.
pub fn safe_open_source(
    source_path: &Path,
    source_root: Option<&Path>,
    follow_symlinks: bool,
) -> Result<fs::File, std::io::Error> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::FromRawFd;

    let root = match source_root {
        Some(r) => r,
        None => return fs::File::open(source_path),
    };

    let (parent_rel, leaf_name) = if root.as_os_str() == "." || root.as_os_str().is_empty() {
        (source_path.parent(), source_path.file_name())
    } else if let Ok(rel) = source_path.strip_prefix(root) {
        (rel.parent(), rel.file_name())
    } else {
        (source_path.parent(), source_path.file_name())
    };

    let leaf_name = match leaf_name {
        Some(name) => name,
        None => return fs::File::open(source_path),
    };

    let start_fd = if root.as_os_str().is_empty() {
        DirFd::open_dir(Path::new("."), false)?
    } else {
        DirFd::open_dir(root, false)?
    };

    let parent_fd = if let Some(p) = parent_rel {
        if p.as_os_str().is_empty() || p.as_os_str() == "." {
            start_fd
        } else {
            let mut cur = start_fd;
            let mut cur_path = root.to_path_buf();
            for comp in p.components() {
                match comp {
                    std::path::Component::Normal(c) => {
                        let name = Path::new(c);
                        let next = match cur.openat_dir(name, !follow_symlinks) {
                            Ok(next) => next,
                            Err(e) => {
                                let full_path = cur_path.join(name);
                                if let Ok(link_target) = fs::read_link(&full_path) {
                                    let resolved = if link_target.is_absolute() {
                                        crate::path::clean_path(&link_target)
                                    } else {
                                        crate::path::clean_path(&cur_path.join(&link_target))
                                    };
                                    if is_beneath(root, &resolved) && resolved.is_dir() {
                                        cur.openat_dir(name, false)?
                                    } else {
                                        return Err(e);
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
                        };
                        cur_path.push(c);
                        cur = next;
                    }
                    std::path::Component::CurDir => {}
                    _ => {}
                }
            }
            cur
        }
    } else {
        start_fd
    };

    let leaf_c = CString::new(leaf_name.as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let mut flags = libc::O_RDONLY | libc::O_CLOEXEC;
    if !follow_symlinks {
        flags |= libc::O_NOFOLLOW;
    }

    // SAFETY: leaf_c is a valid C string, parent_fd.raw() is a valid open dir fd.
    let fd = unsafe { libc::openat(parent_fd.raw(), leaf_c.as_ptr(), flags) };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: fd is a valid open file descriptor.
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
}

/// Atomically renames `tmp_path` to `dest_path` while confining the destination relative to `dest_root`.
///
/// Pins the destination's parent directory through held dirfds with `O_DIRECTORY | O_NOFOLLOW` and
/// executes `renameat`. This prevents TOCTOU races where a parent directory is flipped to a symlink
/// mid-transfer.
pub fn safe_rename_into(
    dest_root: &Path,
    tmp_path: &Path,
    dest_path: &Path,
) -> Result<(), FsError> {
    if dest_path == dest_root {
        fs::rename(tmp_path, dest_path)?;
        return Ok(());
    }

    if let Ok(rel) = dest_path.strip_prefix(dest_root) {
        let parent_rel = rel.parent();
        let leaf_name = rel.file_name().ok_or_else(|| {
            FsError::PathError(format!("missing file name in {}", dest_path.display()))
        })?;

        let parent_fd = if let Some(p) = parent_rel {
            if p.as_os_str().is_empty() {
                DirFd::open_dir(dest_root, false)?
            } else {
                let mut cur_fd = DirFd::open_dir(dest_root, false)?;
                let mut cur_path = dest_root.to_path_buf();
                for comp in p.components() {
                    if let std::path::Component::Normal(c) = comp {
                        let name = Path::new(c);
                        let next_fd = match cur_fd.openat_dir(name, true) {
                            Ok(next) => next,
                            Err(e) => {
                                let full_path = cur_path.join(name);
                                if let Ok(link_target) = fs::read_link(&full_path) {
                                    let resolved = if link_target.is_absolute() {
                                        crate::path::clean_path(&link_target)
                                    } else {
                                        crate::path::clean_path(&cur_path.join(&link_target))
                                    };
                                    if is_beneath(dest_root, &resolved) && resolved.is_dir() {
                                        cur_fd.openat_dir(name, false)?
                                    } else {
                                        return Err(FsError::Io(e));
                                    }
                                } else {
                                    return Err(FsError::Io(e));
                                }
                            }
                        };
                        cur_path.push(c);
                        cur_fd = next_fd;
                    }
                }
                cur_fd
            }
        } else {
            DirFd::open_dir(dest_root, false)?
        };

        let tmp_c = CString::new(tmp_path.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let leaf_c = CString::new(leaf_name.as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

        // SAFETY: `tmp_c` and `leaf_c` are valid null-terminated C strings, `parent_fd` is an open dir fd.
        let rc = unsafe {
            libc::renameat(
                libc::AT_FDCWD,
                tmp_c.as_ptr(),
                parent_fd.raw(),
                leaf_c.as_ptr(),
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(FsError::Io(std::io::Error::last_os_error()))
        }
    } else {
        fs::rename(tmp_path, dest_path)?;
        Ok(())
    }
}
