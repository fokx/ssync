//! Path normalization and trailing slash analysis.

use std::path::{Path, PathBuf};

/// Analyzes whether a path has an explicit trailing slash (which alters rsync semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePath {
    pub path: PathBuf,
    pub has_trailing_slash: bool,
}

impl SourcePath {
    pub fn from_raw(s: &str) -> Self {
        let has_trailing_slash = s.ends_with('/') || s.ends_with("/.");
        let clean = Path::new(s.trim_end_matches('/'));
        Self {
            path: clean.to_path_buf(),
            has_trailing_slash,
        }
    }
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
    }
}
