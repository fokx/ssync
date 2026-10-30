//! ssync: Clean-room, high-performance file synchronization tool in Rust.

use clap::Parser;
use ssync_fs::filter::FilterSet;
use ssync_fs::path::SourcePath;
use ssync_fs::sync::{SyncEngine, SyncOptions};
use ssync_fs::walker::{WalkOptions, Walker};
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ssync",
    author = "ssync contributors",
    disable_version_flag = true,
    about = "Fast, safe, flag-compatible file synchronization tool in Rust"
)]
struct Cli {
    /// Recurse into directories
    #[arg(short = 'r', long = "recursive")]
    recursive: bool,

    /// Archive mode; equals -rlptgoD
    #[arg(short = 'a', long = "archive")]
    archive: bool,

    /// Copy symlinks as symlinks
    #[arg(short = 'l', long = "links")]
    links: bool,

    /// Transform symlink to referent file/dir
    #[arg(short = 'L', long = "copy-links")]
    copy_links: bool,

    /// Transform unsafe symlinks to referent file/dir
    #[arg(long = "copy-unsafe-links")]
    copy_unsafe_links: bool,

    /// Ignore symlinks that point outside the tree
    #[arg(long = "safe-links")]
    safe_links: bool,

    /// Treat symlinked dir on source as dir
    #[arg(short = 'k', long = "copy-dirlinks")]
    copy_dirlinks: bool,

    /// Treat symlinked dir on receiver as dir
    #[arg(short = 'K', long = "keep-dirlinks")]
    keep_dirlinks: bool,

    /// Preserve permissions
    #[arg(short = 'p', long = "perms")]
    perms: bool,

    /// Preserve modification times
    #[arg(short = 't', long = "times")]
    times: bool,

    /// Preserve owner (super-user only)
    #[arg(short = 'o', long = "owner")]
    owner: bool,

    /// Preserve group
    #[arg(short = 'g', long = "group")]
    group: bool,

    /// Same as --devices --specials
    #[arg(short = 'D')]
    devices_specials: bool,

    /// Transfer directories without recursing
    #[arg(short = 'd', long = "dirs")]
    dirs: bool,

    /// Use relative path names
    #[arg(short = 'R', long = "relative")]
    pub relative: bool,

    /// Don't send implied directories with -R
    #[arg(long = "no-implied-dirs")]
    pub no_implied_dirs: bool,

    /// Preserve hard links
    #[arg(short = 'H', long = "hard-links")]
    hard_links: bool,

    /// Verbose output
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-error messages
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Perform a trial run with no changes made
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Extended capability output (used by rsync harness)
    #[arg(short = 'V', action = clap::ArgAction::Count)]
    version_caps: u8,

    /// Print version information
    #[arg(long = "version")]
    version: bool,

    /// Delete extraneous files from destination
    #[arg(long = "delete", alias = "del")]
    delete: bool,

    /// Only update files that already exist on destination
    #[arg(long = "existing")]
    existing: bool,

    /// Output a change-summary for all updates
    #[arg(short = 'i', long = "itemize-changes")]
    itemize_changes: bool,

    /// Exclude files matching PATTERN
    #[arg(long = "exclude", action = clap::ArgAction::Append)]
    exclude: Vec<String>,

    /// Don't exclude files matching PATTERN
    #[arg(long = "include", action = clap::ArgAction::Append)]
    include: Vec<String>,

    /// Bandwidth limit (compatibility placeholder)
    #[arg(long = "bwlimit")]
    bwlimit: Option<String>,

    /// Debug options (compatibility placeholder)
    #[arg(long = "debug")]
    debug: Option<String>,

    /// Disable whole-file updates
    #[arg(long = "no-whole-file")]
    no_whole_file: bool,

    /// Copy whole files without delta-algorithm
    #[arg(short = 'W', long = "whole-file")]
    whole_file: bool,

    /// Source and destination paths
    #[arg(required = false)]
    paths: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    if cli.version {
        println!("ssync version 0.1.0  protocol version 31");
        return;
    }

    if cli.version_caps >= 2 {
        // Output JSON capabilities when -VV is requested (used by rsyncfns in tests)
        println!(r#"{{"ACLs": false, "xattrs": false, "sha256": true, "sha512": true}}"#);
        return;
    } else if cli.version_caps == 1 {
        println!("ssync version 0.1.0  protocol version 31");
        return;
    }

    if cli.paths.len() < 2 {
        eprintln!("ssync: missing source or destination parameters");
        process::exit(1);
    }

    let dest_raw = cli.paths.last().unwrap();
    let sources_raw = &cli.paths[..cli.paths.len() - 1];

    let dest_path = PathBuf::from(dest_raw.trim_end_matches('/'));
    let sources: Vec<SourcePath> = sources_raw
        .iter()
        .map(|s| SourcePath::from_raw(s))
        .collect();

    // Check that source paths exist
    for src in &sources {
        if !src.path.exists() && !src.path.is_symlink() {
            eprintln!(
                "ssync: change_dir \"{}\" failed: No such file or directory (2)",
                src.path.display()
            );
            process::exit(23);
        }
    }

    // Set up filter rules
    let mut filters = FilterSet::new();
    for inc in &cli.include {
        filters.add_include(inc);
    }
    for exc in &cli.exclude {
        filters.add_exclude(exc);
    }

    let recursive = cli.recursive || cli.archive;
    let links = cli.links || cli.archive;
    let perms = cli.perms || cli.archive;
    let times = cli.times || cli.archive;
    let owner = cli.owner || cli.archive;
    let group = cli.group || cli.archive;

    let walk_opts = WalkOptions {
        recursive,
        dirs: cli.dirs,
        links,
        copy_links: cli.copy_links,
        copy_dirlinks: cli.copy_dirlinks,
        copy_unsafe_links: cli.copy_unsafe_links,
        safe_links: cli.safe_links,
        relative: cli.relative,
        no_implied_dirs: cli.no_implied_dirs,
        filters,
    };

    let walker = Walker::new(walk_opts);
    let entries = match walker.scan_sources(&sources) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ssync: error scanning sources: {}", e);
            process::exit(11);
        }
    };

    let sync_opts = SyncOptions {
        dry_run: cli.dry_run,
        verbose: cli.verbose,
        quiet: cli.quiet,
        preserve_perms: perms,
        preserve_times: times,
        preserve_owner: owner,
        preserve_group: group,
        hard_links: cli.hard_links,
        existing: cli.existing,
        delete: cli.delete,
        itemize_changes: cli.itemize_changes,
    };

    let engine = SyncEngine::new(sync_opts);
    if let Err(e) = engine.execute(&entries, &dest_path) {
        eprintln!("ssync: transfer error: {}", e);
        process::exit(11);
    }
}
