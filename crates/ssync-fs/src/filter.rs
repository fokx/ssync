//! Include and exclude filter rule parsing and evaluation.

/// Type of filter rule: include or exclude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleType {
    Include,
    Exclude,
}

/// A parsed filter rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterRule {
    pub rule_type: RuleType,
    pub pattern: String,
    pub dir_only: bool,
    pub anchored: bool,
}

impl FilterRule {
    pub fn parse(spec: &str, rule_type: RuleType) -> Self {
        let mut s = spec;
        let mut dir_only = false;
        let mut anchored = false;

        if s.ends_with('/') {
            dir_only = true;
            s = &s[..s.len() - 1];
        }

        if let Some(stripped) = s.strip_prefix('/') {
            anchored = true;
            s = stripped;
        }

        Self {
            rule_type,
            pattern: s.to_string(),
            dir_only,
            anchored,
        }
    }

    /// Evaluates whether this rule matches the given path and directory flag.
    pub fn matches(&self, rel_path: &str, is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }

        let clean_rel = rel_path.trim_start_matches('/').trim_end_matches('/');

        if self.anchored {
            // Anchored matches starting at the root
            if self.pattern == "*" {
                // Matches any top-level entry
                return !clean_rel.contains('/');
            }
            if clean_rel == self.pattern {
                return true;
            }
            if clean_rel.starts_with(&format!("{}/", self.pattern)) {
                return true;
            }
            simple_glob_match(&self.pattern, clean_rel)
        } else {
            // Unanchored matches against the full path or file basename
            if self.pattern == "*" {
                return true;
            }
            let filename = clean_rel.rsplit('/').next().unwrap_or(clean_rel);
            simple_glob_match(&self.pattern, filename)
                || simple_glob_match(&self.pattern, clean_rel)
        }
    }
}

/// Simple glob matcher supporting `*` and `?`.
fn simple_glob_match(pattern: &str, text: &str) -> bool {
    let p_bytes = pattern.as_bytes();
    let t_bytes = text.as_bytes();
    let (mut p_idx, mut t_idx) = (0, 0);
    let mut star_idx = None;
    let mut match_idx = 0;

    while t_idx < t_bytes.len() {
        if p_idx < p_bytes.len() && (p_bytes[p_idx] == b'?' || p_bytes[p_idx] == t_bytes[t_idx]) {
            p_idx += 1;
            t_idx += 1;
        } else if p_idx < p_bytes.len() && p_bytes[p_idx] == b'*' {
            star_idx = Some(p_idx);
            p_idx += 1;
            match_idx = t_idx;
        } else if let Some(s_idx) = star_idx {
            p_idx = s_idx + 1;
            match_idx += 1;
            t_idx = match_idx;
        } else {
            return false;
        }
    }

    while p_idx < p_bytes.len() && p_bytes[p_idx] == b'*' {
        p_idx += 1;
    }

    p_idx == p_bytes.len()
}

/// A collection of filter rules evaluated in first-matching order.
#[derive(Debug, Clone, Default)]
pub struct FilterSet {
    rules: Vec<FilterRule>,
}

impl FilterSet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_include(&mut self, pattern: &str) {
        self.rules
            .push(FilterRule::parse(pattern, RuleType::Include));
    }

    pub fn add_exclude(&mut self, pattern: &str) {
        self.rules
            .push(FilterRule::parse(pattern, RuleType::Exclude));
    }

    /// Evaluates rules for the given path. Returns true if included, false if excluded.
    pub fn is_included(&self, rel_path: &str, is_dir: bool) -> bool {
        for rule in &self.rules {
            if rule.matches(rel_path, is_dir) {
                return rule.rule_type == RuleType::Include;
            }
        }
        // Default is included
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_matching() {
        let mut filters = FilterSet::new();
        filters.add_include("*/");
        filters.add_exclude("*");

        // Directories should be included by `*/`
        assert!(filters.is_included("some_dir", true));
        assert!(filters.is_included("some_dir/sub", true));

        // Files should be excluded by `*`
        assert!(!filters.is_included("some_file", false));
        assert!(!filters.is_included("some_dir/file.txt", false));
    }

    #[test]
    fn test_anchored_filter() {
        let mut filters = FilterSet::new();
        filters.add_include("/down/");
        filters.add_exclude("/*");

        // `/down/` matches top-level down directory and sub-items
        assert!(filters.is_included("down", true));
        assert!(filters.is_included("down/file", false));

        // Other top-level items are excluded by `/*`
        assert!(!filters.is_included("other", true));
        assert!(!filters.is_included("other_file", false));
    }
}
