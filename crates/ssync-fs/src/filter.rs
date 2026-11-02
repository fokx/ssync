//! Include and exclude filter rule parsing and evaluation.

use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

/// Default patterns ignored by CVS and rsync's `-C` / `--cvs-exclude`.
pub const CVS_IGNORE_PATTERNS: &[&str] = &[
    "RCS",
    "SCCS",
    "CVS",
    "CVS.adm",
    "RCSLOG",
    "cvslog.*",
    "tags",
    "TAGS",
    ".make.state",
    ".nse_depinfo",
    "*~",
    "#*",
    ".#*",
    ",*",
    "_$*",
    "*$",
    "*.old",
    "*.bak",
    "*.BAK",
    "*.orig",
    "*.rej",
    ".del-*",
    "*.a",
    "*.olb",
    "*.o",
    "*.obj",
    "*.so",
    "*.exe",
    "*.Z",
    "*.elc",
    "*.ln",
    "core",
    ".svn/",
    ".git/",
    ".hg/",
    ".bzr/",
];

/// Type of filter rule action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    Include,
    Exclude,
    Protect,
    Risk,
    Clear,
}

/// Scope of sides where the rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSide {
    Both,
    Sender,
    Receiver,
}

/// Provenance tracking for filter rules to prevent information disclosure in error diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleOrigin {
    Arg,
    ArgFile { path: String, line: usize },
    FileNamedAt { parent_loc: String },
    UnknownFile,
}

impl RuleOrigin {
    pub fn is_from_file(&self) -> bool {
        !matches!(self, RuleOrigin::Arg)
    }

    pub fn rule_text(&self, text: &str) -> String {
        if !self.is_from_file() {
            return text.to_string();
        }
        format!("<rule from {}>", self.where_desc())
    }

    pub fn where_desc(&self) -> String {
        match self {
            RuleOrigin::Arg => "arg".to_string(),
            RuleOrigin::ArgFile { path, line } => {
                if *line > 0 {
                    format!("{} line {}", path, line)
                } else {
                    path.clone()
                }
            }
            RuleOrigin::FileNamedAt { parent_loc } => {
                format!("a file named at {}", parent_loc)
            }
            RuleOrigin::UnknownFile => "a file read earlier".to_string(),
        }
    }
}

/// A parsed individual filter rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterRule {
    pub action: RuleAction,
    pub side: RuleSide,
    pub pattern: String,
    pub dir_only: bool,
    pub anchored: bool,
    pub match_any_depth: bool,
    pub recursive_subtree: bool,
    pub negate: bool,
    pub inherit: bool,
    pub base_dir: PathBuf,
    pub origin: RuleOrigin,
}

/// A per-directory merge file specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirMergeRule {
    pub filename: String,
    pub side: RuleSide,
    pub inherit: bool,
    pub exclude_self: bool,
    pub word_split: bool,
    pub cvs_ignore: bool,
    pub no_prefixes: bool,
    pub default_action: RuleAction,
    pub base_dir: PathBuf,
    pub origin: RuleOrigin,
}

impl FilterRule {
    pub fn format_for_trace(&self) -> String {
        if self.origin.is_from_file() {
            self.origin.rule_text(&self.pattern)
        } else {
            let mut s = self.pattern.clone();
            if self.dir_only {
                s.push('/');
            }
            s
        }
    }
    /// Evaluates whether this rule matches the given path and directory flag.
    /// `rel_path` is relative to the transfer root.
    pub fn matches(&self, rel_path: &str, is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return self.negate;
        }

        let clean_path = rel_path.trim_matches('/');

        // If the rule is associated with a base directory (from a dir-merge file),
        // the target path must reside under base_dir.
        let target = if !self.base_dir.as_os_str().is_empty() {
            let base_str = self.base_dir.to_string_lossy();
            let base_clean = base_str.trim_matches('/');
            if clean_path == base_clean {
                ""
            } else if let Some(stripped) = clean_path.strip_prefix(&format!("{}/", base_clean)) {
                if !self.inherit && stripped.contains('/') {
                    return false;
                }
                stripped
            } else {
                return false;
            }
        } else {
            clean_path
        };

        let raw_match = if self.recursive_subtree && self.pattern.ends_with("/***") {
            let dir_pat = &self.pattern[..self.pattern.len() - 4];
            let star_pat = format!("{}/**", dir_pat);
            let matches_dir = |target_slice: &str| {
                wildmatch(dir_pat, target_slice) || wildmatch(&star_pat, target_slice)
            };

            if self.anchored {
                matches_dir(target)
            } else if matches_dir(target) {
                true
            } else {
                let mut sub = target;
                let mut matched = false;
                while let Some(idx) = sub.find('/') {
                    sub = &sub[idx + 1..];
                    if matches_dir(sub) {
                        matched = true;
                        break;
                    }
                }
                matched
            }
        } else if self.match_any_depth {
            let basename = target.rsplit('/').next().unwrap_or(target);
            wildmatch(&self.pattern, basename)
        } else if self.anchored {
            wildmatch(&self.pattern, target)
        } else if wildmatch(&self.pattern, target) {
            true
        } else {
            let mut sub = target;
            let mut matched = false;
            while let Some(idx) = sub.find('/') {
                sub = &sub[idx + 1..];
                if wildmatch(&self.pattern, sub) {
                    matched = true;
                    break;
                }
            }
            matched
        };

        if self.negate { !raw_match } else { raw_match }
    }
}

/// Performs rsync shell-style wildmatching (`wildmatch.c` algorithm).
pub fn wildmatch(pattern: &str, text: &str) -> bool {
    dowild(pattern.as_bytes(), text.as_bytes())
}

fn dowild(mut pat: &[u8], mut text: &[u8]) -> bool {
    while !pat.is_empty() {
        match pat[0] {
            b'\\' => {
                if pat.len() > 1 {
                    pat = &pat[1..];
                }
                if text.is_empty() || text[0] != pat[0] {
                    return false;
                }
                pat = &pat[1..];
                text = &text[1..];
            }
            b'?' => {
                if text.is_empty() || text[0] == b'/' {
                    return false;
                }
                pat = &pat[1..];
                text = &text[1..];
            }
            b'*' => {
                let mut star_count = 0;
                while !pat.is_empty() && pat[0] == b'*' {
                    star_count += 1;
                    pat = &pat[1..];
                }

                if pat.is_empty() {
                    if star_count == 1 {
                        return !text.contains(&b'/');
                    }
                    return true;
                }

                if star_count == 1 {
                    while !text.is_empty() && text[0] != b'/' {
                        if dowild(pat, text) {
                            return true;
                        }
                        text = &text[1..];
                    }
                    return dowild(pat, text);
                } else {
                    if pat[0] == b'/' && dowild(&pat[1..], text) {
                        return true;
                    }
                    while !text.is_empty() {
                        if dowild(pat, text) {
                            return true;
                        }
                        text = &text[1..];
                    }
                    return dowild(pat, text);
                }
            }
            b'[' => {
                if text.is_empty() {
                    return false;
                }
                let t_char = text[0];
                if t_char == b'/' {
                    return false;
                }

                let (matched, new_pat) = match_char_class(&pat[1..], t_char);
                if !matched {
                    return false;
                }
                pat = new_pat;
                text = &text[1..];
            }
            ch => {
                if text.is_empty() || text[0] != ch {
                    return false;
                }
                pat = &pat[1..];
                text = &text[1..];
            }
        }
    }

    text.is_empty()
}

fn match_char_class(pat: &[u8], ch: u8) -> (bool, &[u8]) {
    if pat.is_empty() {
        return (false, pat);
    }

    let mut idx = 0;
    let negated = if pat[idx] == b'!' || pat[idx] == b'^' {
        idx += 1;
        true
    } else {
        false
    };

    let mut matched = false;
    let mut prev_char: Option<u8> = None;

    while idx < pat.len() {
        let curr = pat[idx];
        if curr == b']' && prev_char.is_some() {
            idx += 1;
            break;
        }

        if curr == b'-' && prev_char.is_some() && idx + 1 < pat.len() && pat[idx + 1] != b']' {
            let start = prev_char.unwrap();
            let end = pat[idx + 1];
            if ch >= start && ch <= end {
                matched = true;
            }
            idx += 2;
            prev_char = None;
            continue;
        }

        if curr == ch {
            matched = true;
        }
        prev_char = Some(curr);
        idx += 1;
    }

    let res = if negated { !matched } else { matched };
    (res, &pat[idx..])
}

/// A collection of filter rules evaluated in first-matching order.
#[derive(Debug, Clone, Default)]
pub struct FilterSet {
    rules: Vec<FilterRule>,
    dir_merge_rules: Vec<DirMergeRule>,
    pub debug_filter: bool,
}

impl FilterSet {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            dir_merge_rules: Vec::new(),
            debug_filter: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.dir_merge_rules.is_empty()
    }

    pub fn add_include(&mut self, spec: &str) {
        self.parse_and_add_rule(
            spec,
            RuleAction::Include,
            false,
            Path::new(""),
            &RuleOrigin::Arg,
        );
    }

    pub fn add_exclude(&mut self, spec: &str) {
        self.parse_and_add_rule(
            spec,
            RuleAction::Exclude,
            false,
            Path::new(""),
            &RuleOrigin::Arg,
        );
    }

    /// Parses and adds a filter rule from a `--filter` or `-f` argument.
    pub fn add_filter_arg(&mut self, arg: &str) -> Result<(), String> {
        self.parse_filter_rule(arg, Path::new(""), Path::new(""), 0, &RuleOrigin::Arg)
    }

    /// Loads rules from a merge file specified by `--filter=. FILE` or `. FILE`.
    pub fn load_filter_file(
        &mut self,
        path: &Path,
        fs_dir: &Path,
        rel_dir: &Path,
        depth: usize,
        origin: &RuleOrigin,
    ) -> Result<(), String> {
        if depth >= 32 {
            return Err("merge-file include depth limit (32) exceeded".to_string());
        }
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                let target_desc = match origin {
                    RuleOrigin::Arg => format!("'{}'", path.display()),
                    RuleOrigin::ArgFile { path: p, line: l } => {
                        format!("<rule from a file named at {} line {}>", p, l)
                    }
                    RuleOrigin::FileNamedAt { parent_loc } => {
                        format!("<rule from a file named at {}>", parent_loc)
                    }
                    RuleOrigin::UnknownFile => "<rule from a file read earlier>".to_string(),
                };
                eprintln!("cannot open filter file {}: {}", target_desc, e);
                std::process::exit(11);
            }
        };
        let reader = io::BufReader::new(file);
        for (idx, line) in reader.lines().enumerate() {
            let line_no = idx + 1;
            let line = line.map_err(|e| format!("failed reading filter file: {}", e))?;
            let trimmed = line.trim_end_matches(['\r', '\n']).trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if trimmed == "!" || trimmed == "clear" {
                self.rules.clear();
                continue;
            }
            let cur_origin = match origin {
                RuleOrigin::Arg => RuleOrigin::ArgFile {
                    path: path.to_string_lossy().to_string(),
                    line: line_no,
                },
                RuleOrigin::ArgFile { path: p, .. } => RuleOrigin::ArgFile {
                    path: p.clone(),
                    line: line_no,
                },
                RuleOrigin::FileNamedAt { parent_loc } => RuleOrigin::FileNamedAt {
                    parent_loc: parent_loc.clone(),
                },
                RuleOrigin::UnknownFile => RuleOrigin::UnknownFile,
            };
            if let Err(e) = self.parse_filter_rule(trimmed, fs_dir, rel_dir, depth, &cur_origin) {
                eprintln!("{}", e);
                if e.contains("depth limit") {
                    std::process::exit(11);
                } else {
                    std::process::exit(1);
                }
            }
        }
        Ok(())
    }

    /// Loads rules from an exclude/include file (e.g. `--exclude-from` or `--include-from`).
    pub fn load_file(&mut self, path: &Path, default_action: RuleAction) -> io::Result<()> {
        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);
        for (idx, line) in reader.lines().enumerate() {
            let line_no = idx + 1;
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if trimmed == "!" || trimmed == "clear" {
                self.rules.clear();
                continue;
            }
            let cur_origin = RuleOrigin::ArgFile {
                path: path.to_string_lossy().to_string(),
                line: line_no,
            };
            if self
                .parse_filter_rule(trimmed, Path::new(""), Path::new(""), 0, &cur_origin)
                .is_err()
            {
                self.parse_and_add_rule(trimmed, default_action, false, Path::new(""), &cur_origin);
            }
        }
        Ok(())
    }

    /// Loads rules from standard input (when `-` is passed as file).
    pub fn load_stdin(&mut self, default_action: RuleAction) -> io::Result<()> {
        let stdin = io::stdin();
        let reader = stdin.lock();
        for (idx, line) in reader.lines().enumerate() {
            let line_no = idx + 1;
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if trimmed == "!" || trimmed == "clear" {
                self.rules.clear();
                continue;
            }
            let cur_origin = RuleOrigin::ArgFile {
                path: "-".to_string(),
                line: line_no,
            };
            if self
                .parse_filter_rule(trimmed, Path::new(""), Path::new(""), 0, &cur_origin)
                .is_err()
            {
                self.parse_and_add_rule(trimmed, default_action, false, Path::new(""), &cur_origin);
            }
        }
        Ok(())
    }

    /// Adds the standard CVS exclude rules.
    pub fn add_cvs_excludes(&mut self) {
        if !self.dir_merge_rules.iter().any(|r| r.cvs_ignore) {
            self.dir_merge_rules.push(DirMergeRule {
                filename: ".cvsignore".to_string(),
                inherit: false,
                side: RuleSide::Both,
                exclude_self: true,
                word_split: true,
                cvs_ignore: true,
                no_prefixes: false,
                default_action: RuleAction::Exclude,
                base_dir: PathBuf::new(),
                origin: RuleOrigin::Arg,
            });
        }
        for pat in CVS_IGNORE_PATTERNS {
            self.add_exclude(pat);
        }
        if let Ok(cvsignore) = std::env::var("CVSIGNORE") {
            for word in cvsignore.split_whitespace() {
                self.add_exclude(word);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let home_cvs = Path::new(&home).join(".cvsignore");
            if let Ok(file) = fs::File::open(home_cvs) {
                let reader = io::BufReader::new(file);
                for line in reader.lines().map_while(Result::ok) {
                    for word in line.split_whitespace() {
                        self.add_exclude(word);
                    }
                }
            }
        }
    }

    /// Parses a single rule line from a file or command line.
    pub fn parse_filter_line(
        &mut self,
        line: &str,
        _default_action: RuleAction,
        base_dir: &Path,
    ) -> Result<(), String> {
        self.parse_filter_rule(line, base_dir, base_dir, 0, &RuleOrigin::Arg)
    }

    /// Parses a filter rule with prefix and modifiers.
    pub fn parse_filter_rule(
        &mut self,
        spec: &str,
        fs_dir: &Path,
        rel_dir: &Path,
        depth: usize,
        origin: &RuleOrigin,
    ) -> Result<(), String> {
        if spec.len() >= 4096 {
            if origin.is_from_file() {
                eprintln!("discarding over-long filter: {}", origin.rule_text(spec));
            } else {
                eprintln!("discarding over-long filter: {}", spec);
            }
            return Ok(());
        }

        let s = spec.trim_end_matches(['\r', '\n']).trim_start();
        if s.is_empty() {
            return Ok(());
        }

        if s == "!" || s == "clear" {
            self.rules.clear();
            return Ok(());
        }

        if s == ":C" {
            self.dir_merge_rules.push(DirMergeRule {
                filename: ".cvsignore".to_string(),
                side: RuleSide::Both,
                inherit: false,
                exclude_self: true,
                word_split: true,
                cvs_ignore: true,
                no_prefixes: false,
                default_action: RuleAction::Exclude,
                base_dir: rel_dir.to_path_buf(),
                origin: if origin.is_from_file() {
                    RuleOrigin::UnknownFile
                } else {
                    RuleOrigin::Arg
                },
            });
            return Ok(());
        }

        if s == "-C" {
            self.add_cvs_excludes();
            return Ok(());
        }

        // Check for long-form keywords or short prefixes
        let (prefix, rest) = if let Some(r) = s.strip_prefix("dir-merge") {
            (":", r)
        } else if let Some(r) = s.strip_prefix("merge") {
            (".", r)
        } else if let Some(r) = s.strip_prefix("exclude") {
            ("-", r)
        } else if let Some(r) = s.strip_prefix("include") {
            ("+", r)
        } else if let Some(r) = s.strip_prefix("protect") {
            ("P", r)
        } else if let Some(r) = s.strip_prefix("risk") {
            ("R", r)
        } else if let Some(r) = s.strip_prefix("hide") {
            ("H", r)
        } else if let Some(r) = s.strip_prefix("show") {
            ("S", r)
        } else {
            let ch = s.chars().next().unwrap();
            match ch {
                ':' | '.' | '-' | '+' | 'P' | 'R' | 'H' | 'S' => (&s[..1], &s[1..]),
                _ => {
                    let desc = origin.rule_text(s);
                    return Err(format!("Unknown filter rule: {}", desc));
                }
            }
        };

        let mut modifiers = "";
        let mut pattern = rest;

        if let Some(idx) = rest.find(['_', ' ']) {
            modifiers = &rest[..idx];
            pattern = &rest[idx + 1..];
        } else if rest.starts_with(['-', '+', 'C']) {
            modifiers = rest;
            pattern = "";
        }

        if !origin.is_from_file()
            && self.debug_filter
            && (pattern.ends_with(' ') || pattern.ends_with('\t'))
        {
            println!("[client] add_rule({}) -- CAUTION: trailing whitespace!", s);
        }

        let mut side = RuleSide::Both;
        let mut inherit = true;
        let mut exclude_self = false;
        let mut word_split = false;
        let mut cvs_ignore = false;
        let mut negate = false;
        let mut no_prefixes = false;
        let mut default_action = RuleAction::Exclude;

        for m in modifiers.chars() {
            match m {
                's' => side = RuleSide::Sender,
                'r' => side = RuleSide::Receiver,
                'n' => inherit = false,
                'e' => exclude_self = true,
                'w' => word_split = true,
                '!' => negate = true,
                '-' => {
                    no_prefixes = true;
                    default_action = RuleAction::Exclude;
                }
                '+' => {
                    no_prefixes = true;
                    default_action = RuleAction::Include;
                }
                'C' => {
                    cvs_ignore = true;
                    word_split = true;
                    inherit = false;
                }
                '/' => {} // Anchored to transfer root for dir-merge
                ',' | ' ' | '_' => {}
                _ => {
                    let desc = origin.rule_text(s);
                    return Err(format!("invalid modifier in filter rule: {}", desc));
                }
            }
        }

        match prefix {
            ":" => {
                let raw_fname = if pattern.is_empty() {
                    if cvs_ignore {
                        ".cvsignore"
                    } else {
                        ".rsync-filter"
                    }
                } else {
                    pattern
                };
                let fname = raw_fname.strip_prefix('/').unwrap_or(raw_fname);
                let dm_origin = if origin.is_from_file() {
                    RuleOrigin::UnknownFile
                } else {
                    RuleOrigin::Arg
                };
                self.dir_merge_rules.push(DirMergeRule {
                    filename: fname.to_string(),
                    side,
                    inherit,
                    exclude_self,
                    word_split,
                    cvs_ignore,
                    no_prefixes,
                    default_action,
                    base_dir: rel_dir.to_path_buf(),
                    origin: dm_origin,
                });
                if exclude_self {
                    let base_pattern = fname.rsplit('/').next().unwrap_or(fname);
                    let self_rule =
                        Self::parse_rule(base_pattern, RuleAction::Exclude, false, rel_dir, origin);
                    self.rules.push(self_rule);
                }
            }
            "." => {
                if depth >= 32 {
                    return Err("merge-file include depth limit (32) exceeded".to_string());
                }
                if pattern == "-" {
                    self.load_stdin(RuleAction::Exclude)
                        .map_err(|e| format!("failed to read stdin filter rules: {}", e))?;
                } else {
                    let file_to_open = if fs_dir.as_os_str().is_empty() {
                        PathBuf::from(pattern)
                    } else {
                        fs_dir.join(pattern)
                    };
                    let child_origin = match origin {
                        RuleOrigin::Arg => RuleOrigin::ArgFile {
                            path: file_to_open.to_string_lossy().to_string(),
                            line: 1,
                        },
                        RuleOrigin::ArgFile { path: p, line: l } => RuleOrigin::FileNamedAt {
                            parent_loc: format!("{} line {}", p, l),
                        },
                        RuleOrigin::FileNamedAt { parent_loc } => RuleOrigin::FileNamedAt {
                            parent_loc: parent_loc.clone(),
                        },
                        RuleOrigin::UnknownFile => RuleOrigin::UnknownFile,
                    };
                    self.load_filter_file(
                        &file_to_open,
                        fs_dir,
                        rel_dir,
                        depth + 1,
                        &child_origin,
                    )?;
                }
            }
            "+" => {
                self.parse_and_add_rule(pattern, RuleAction::Include, negate, rel_dir, origin);
            }
            "-" => {
                if (cvs_ignore && pattern.is_empty()) || pattern == "C" {
                    self.add_cvs_excludes();
                } else {
                    self.parse_and_add_rule(pattern, RuleAction::Exclude, negate, rel_dir, origin);
                }
            }
            "P" => {
                self.parse_and_add_rule(pattern, RuleAction::Protect, negate, rel_dir, origin);
            }
            "R" => {
                self.parse_and_add_rule(pattern, RuleAction::Risk, negate, rel_dir, origin);
            }
            "H" => {
                let mut rule =
                    Self::parse_rule(pattern, RuleAction::Exclude, negate, rel_dir, origin);
                rule.side = RuleSide::Sender;
                self.rules.push(rule);
            }
            "S" => {
                let mut rule =
                    Self::parse_rule(pattern, RuleAction::Include, negate, rel_dir, origin);
                rule.side = RuleSide::Sender;
                self.rules.push(rule);
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    fn parse_and_add_rule(
        &mut self,
        spec: &str,
        action: RuleAction,
        negate: bool,
        base_dir: &Path,
        origin: &RuleOrigin,
    ) {
        let rule = Self::parse_rule(spec, action, negate, base_dir, origin);
        self.rules.push(rule);
    }

    fn parse_rule(
        spec: &str,
        action: RuleAction,
        negate: bool,
        base_dir: &Path,
        origin: &RuleOrigin,
    ) -> FilterRule {
        let mut s = spec.trim_end_matches(['\r', '\n']).trim_start();
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

        let recursive_subtree = s.ends_with("***");
        let has_internal_slash = s.contains('/') || s.contains("**");
        let match_any_depth = !anchored && !has_internal_slash;

        FilterRule {
            action,
            side: RuleSide::Both,
            pattern: s.to_string(),
            dir_only,
            anchored,
            match_any_depth,
            recursive_subtree,
            negate,
            inherit: true,
            base_dir: base_dir.to_path_buf(),
            origin: origin.clone(),
        }
    }

    /// Evaluates rules for the given path on the sender side.
    pub fn is_included(&self, rel_path: &str, is_dir: bool) -> bool {
        self.is_included_side(rel_path, is_dir, RuleSide::Sender)
    }

    /// Evaluates rules for the given path on the receiver side.
    pub fn is_included_on_receiver(&self, rel_path: &str, is_dir: bool) -> bool {
        self.is_included_side(rel_path, is_dir, RuleSide::Receiver)
    }

    pub fn check_match(
        &self,
        rel_path: &str,
        is_dir: bool,
        current_side: RuleSide,
    ) -> Option<(bool, &FilterRule)> {
        for rule in &self.rules {
            if rule.side != RuleSide::Both && rule.side != current_side {
                continue;
            }
            if rule.matches(rel_path, is_dir) {
                let included = match rule.action {
                    RuleAction::Include | RuleAction::Risk => true,
                    RuleAction::Exclude | RuleAction::Protect => false,
                    RuleAction::Clear => true,
                };
                return Some((included, rule));
            }
        }
        None
    }

    pub fn is_included_side(&self, rel_path: &str, is_dir: bool, current_side: RuleSide) -> bool {
        self.check_match(rel_path, is_dir, current_side)
            .is_none_or(|(inc, _)| inc)
    }

    /// Evaluates whether a destination path is protected from deletion.
    pub fn is_protected_from_deletion(&self, rel_path: &str, is_dir: bool) -> bool {
        for rule in &self.rules {
            if rule.side == RuleSide::Sender {
                continue;
            }
            if rule.matches(rel_path, is_dir) {
                match rule.action {
                    RuleAction::Protect => return true,
                    RuleAction::Risk => return false,
                    _ => {}
                }
            }
        }
        false
    }

    /// Checks if a directory contains any active dir-merge files, and returns their rules.
    pub fn load_dir_merge_rules(
        &mut self,
        dir_path: &Path,
        rel_dir: &Path,
        current_side: RuleSide,
    ) -> Vec<FilterRule> {
        let mut new_rules = Vec::new();
        let mut idx = 0;

        while idx < self.dir_merge_rules.len() {
            let dm = self.dir_merge_rules[idx].clone();
            idx += 1;

            if current_side == RuleSide::Receiver && dm.side == RuleSide::Sender {
                continue;
            }
            if current_side == RuleSide::Sender && dm.side == RuleSide::Receiver {
                continue;
            }

            let merge_file_path = dir_path.join(&dm.filename);
            if merge_file_path.as_os_str().len() >= 4096 {
                eprintln!(
                    "merge-file name overflows: {}",
                    dm.origin.rule_text(&dm.filename)
                );
                continue;
            }

            if merge_file_path.is_file() {
                if let Ok(file) = fs::File::open(&merge_file_path) {
                    let mut file_rules = Vec::new();
                    let reader = io::BufReader::new(file);
                    for (idx, line) in reader.lines().map_while(Result::ok).enumerate() {
                        let line_no = idx + 1;
                        let trimmed = line.trim();
                        if trimmed.is_empty()
                            || trimmed.starts_with('#')
                            || trimmed.starts_with(';')
                        {
                            continue;
                        }

                        if trimmed == "!" || trimmed == "clear" {
                            file_rules.clear();
                            continue;
                        }

                        let cur_origin = match &dm.origin {
                            RuleOrigin::Arg => RuleOrigin::ArgFile {
                                path: dm.filename.clone(),
                                line: line_no,
                            },
                            RuleOrigin::ArgFile { path, .. } => RuleOrigin::ArgFile {
                                path: path.clone(),
                                line: line_no,
                            },
                            RuleOrigin::FileNamedAt { parent_loc } => RuleOrigin::FileNamedAt {
                                parent_loc: parent_loc.clone(),
                            },
                            RuleOrigin::UnknownFile => RuleOrigin::UnknownFile,
                        };

                        if dm.no_prefixes {
                            let mut rule = Self::parse_rule(
                                trimmed,
                                dm.default_action,
                                false,
                                rel_dir,
                                &cur_origin,
                            );
                            rule.inherit = dm.inherit;
                            file_rules.push(rule);
                        } else if dm.word_split {
                            for word in trimmed.split_whitespace() {
                                if word == "!" || word == "clear" {
                                    file_rules.clear();
                                    continue;
                                }
                                let mut rule = Self::parse_rule(
                                    word,
                                    RuleAction::Exclude,
                                    false,
                                    rel_dir,
                                    &cur_origin,
                                );
                                rule.inherit = dm.inherit;
                                file_rules.push(rule);
                            }
                        } else {
                            let mut local_set = FilterSet::new();
                            local_set.debug_filter = self.debug_filter;
                            match local_set.parse_filter_rule(
                                trimmed,
                                dir_path,
                                rel_dir,
                                0,
                                &cur_origin,
                            ) {
                                Ok(()) => {
                                    for mut r in local_set.rules {
                                        r.inherit = dm.inherit;
                                        file_rules.push(r);
                                    }
                                    for dm_rule in local_set.dir_merge_rules {
                                        self.dir_merge_rules.push(dm_rule);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                    }
                    new_rules.extend(file_rules);
                }
            }
        }

        new_rules
    }

    /// Extends this FilterSet with new rules.
    pub fn extend_rules(&mut self, rules: Vec<FilterRule>) {
        self.rules.extend(rules);
    }

    /// Prepends new rules to the front of this FilterSet so local/newest rules take precedence.
    pub fn prepend_rules(&mut self, rules: Vec<FilterRule>) {
        self.rules.splice(0..0, rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildmatch_basic() {
        assert!(wildmatch("foo", "foo"));
        assert!(!wildmatch("foo", "bar"));
        assert!(wildmatch("foo*bar", "fooxyzbar"));
        assert!(!wildmatch("foo*bar", "foo/xyz/bar"));
        assert!(wildmatch("foo**bar", "foo/xyz/bar"));
        assert!(wildmatch("*.c", "main.c"));
        assert!(!wildmatch("*.c", "main.h"));
        assert!(wildmatch("test?", "test1"));
        assert!(!wildmatch("test?", "test12"));
    }

    #[test]
    fn test_wildmatch_char_classes() {
        assert!(wildmatch("[a-z]", "m"));
        assert!(!wildmatch("[a-z]", "M"));
        assert!(wildmatch("[!a-z]", "M"));
        assert!(wildmatch("[0-9]", "5"));
        assert!(!wildmatch("[0-9]", "x"));
        assert!(wildmatch("file[2-9]", "file4"));
        assert!(!wildmatch("file[2-9]", "file1"));
    }

    #[test]
    fn test_filter_matching() {
        let mut filters = FilterSet::new();
        filters.add_include("*/");
        filters.add_exclude("*");

        assert!(filters.is_included("dir", true));
        assert!(!filters.is_included("file.txt", false));
    }

    #[test]
    fn test_anchored_filter() {
        let mut filters = FilterSet::new();
        filters.add_exclude("/foo");

        assert!(!filters.is_included("foo", false));
        assert!(filters.is_included("bar/foo", false));
    }

    #[test]
    fn test_recursive_wildcards() {
        let mut filters = FilterSet::new();
        filters.add_include("**/bar");
        filters.add_exclude("/bar");
        filters.add_exclude("new/keep/**");
        filters.add_exclude("new/lose/***");

        assert!(filters.is_included("bar", true));
        assert!(filters.is_included("down/to/bar", true));
        assert!(!filters.is_included("new/keep/this", false));
        assert!(!filters.is_included("new/lose", true));
        assert!(!filters.is_included("new/lose/sub/file", false));
    }

    #[test]
    fn test_negated_exclude_rule() {
        let mut filters = FilterSet::new();
        let _ = filters.add_filter_arg("exclude,! */");

        // Directories should NOT be excluded
        assert!(filters.is_included("dir", true));
        // Files should be excluded
        assert!(!filters.is_included("file.txt", false));
    }
}
