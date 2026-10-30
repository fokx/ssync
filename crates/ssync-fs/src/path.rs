//! Path normalization, trailing slash analysis, and relative path resolution.

use std::path::{Path, PathBuf};

/// Analyzes whether a path has an explicit trailing slash and resolves relative cut points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePath {
    pub raw: String,
    pub path: PathBuf,
    pub has_trailing_slash: bool,
}

impl SourcePath {
    pub fn from_raw(s: &str) -> Self {
        let has_trailing_slash = s.ends_with('/') || s.ends_with("/.");
        let trimmed = s.trim_end_matches('/');
        let path = if trimmed.is_empty() {
            PathBuf::from("/")
        } else {
            PathBuf::from(trimmed)
        };
        Self {
            raw: s.to_string(),
            path,
            has_trailing_slash,
        }
    }

    /// Computes the relative path component for `--relative` (`-R`).
    ///
    /// In rsync:
    /// - If the raw argument contains `/./`, the portion after the `/./` is the relative path.
    /// - If the raw argument starts with `./`, the portion after `./` is the relative path.
    /// - If the path is absolute (starts with `/`), the leading slash is stripped.
    /// - Otherwise, the relative path is the path itself.
    pub fn relative_cut_path(&self) -> PathBuf {
        let raw = &self.raw;
        if let Some(idx) = raw.find("/./") {
            let after = &raw[idx + 3..];
            PathBuf::from(after.trim_end_matches('/'))
        } else if let Some(stripped) = raw.strip_prefix("./") {
            PathBuf::from(stripped.trim_end_matches('/'))
        } else if let Some(stripped) = raw.strip_prefix('/') {
            PathBuf::from(stripped.trim_end_matches('/'))
        } else {
            PathBuf::from(raw.trim_end_matches('/'))
        }
    }

    /// Returns the anchor directory for `--relative` if one is explicitly specified.
    pub fn anchor_path(&self) -> Option<PathBuf> {
        let raw = &self.raw;
        if let Some(idx) = raw.find("/./") {
            let before = &raw[..idx];
            Some(if before.is_empty() {
                PathBuf::from("/")
            } else {
                PathBuf::from(before)
            })
        } else if raw.starts_with("./") {
            Some(PathBuf::from("."))
        } else {
            None
        }
    }

    /// Returns the implied ancestor directory components for an -R transfer path.
    ///
    /// E.g. for relative path "a/b/c/file", the implied ancestors are ["a", "a/b", "a/b/c"].
    pub fn implied_ancestors(&self) -> Vec<PathBuf> {
        let rel = self.relative_cut_path();
        let mut ancestors = Vec::new();
        if let Some(parent) = rel.parent() {
            let mut current = PathBuf::new();
            for comp in parent.components() {
                current.push(comp);
                ancestors.push(current.clone());
            }
        }
        ancestors
    }
}

/// Normalizes a path by removing redundant `./` and `//` sequences.
pub fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trailing_slash() {
        let p1 = SourcePath::from_raw("src/");
        assert!(p1.has_trailing_slash);
        assert_eq!(p1.path, PathBuf::from("src"));

        let p2 = SourcePath::from_raw("src");
        assert!(!p2.has_trailing_slash);
        assert_eq!(p2.path, PathBuf::from("src"));

        let p3 = SourcePath::from_raw("/");
        assert!(p3.has_trailing_slash);
        assert_eq!(p3.path, PathBuf::from("/"));

        let p4 = SourcePath::from_raw("dir/.");
        assert!(p4.has_trailing_slash);
    }

    #[test]
    fn test_relative_cut_path() {
        let p1 = SourcePath::from_raw("/extra/./down/3/deep/file");
        assert_eq!(p1.relative_cut_path(), PathBuf::from("down/3/deep/file"));

        let p2 = SourcePath::from_raw("./down/3/deep");
        assert_eq!(p2.relative_cut_path(), PathBuf::from("down/3/deep"));

        let p3 = SourcePath::from_raw("/abs/path/to/file");
        assert_eq!(p3.relative_cut_path(), PathBuf::from("abs/path/to/file"));

        let p4 = SourcePath::from_raw("b/c/file");
        assert_eq!(p4.relative_cut_path(), PathBuf::from("b/c/file"));
    }

    #[test]
    fn test_implied_ancestors() {
        let p = SourcePath::from_raw("b/c/file");
        assert_eq!(
            p.implied_ancestors(),
            vec![PathBuf::from("b"), PathBuf::from("b/c")]
        );
    }
}
