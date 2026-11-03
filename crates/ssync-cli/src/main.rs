//! ssync: Clean-room, high-performance file synchronization tool in Rust.

use clap::Parser;
use ssync_fs::FsError;
use ssync_fs::filter::{FilterSet, RuleAction};
use ssync_fs::path::SourcePath;
use ssync_fs::sync::{SyncEngine, SyncOptions};
use ssync_fs::walker::{WalkOptions, Walker};
use std::path::{Path, PathBuf};
use std::process;

mod wire;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ssync",
    author = "ssync contributors",
    disable_version_flag = true,
    disable_help_flag = true,
    about = "Fast, safe, flag-compatible file synchronization tool in Rust"
)]
struct Cli {
    /// Show this help message
    #[arg(long = "help")]
    help: bool,
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

    /// Preserve executability
    #[arg(short = 'E', long = "executability")]
    executability: bool,

    /// Preserve modification times
    #[arg(short = 't', long = "times")]
    times: bool,

    /// Omit directories from --times
    #[arg(short = 'O', long = "omit-dir-times")]
    omit_dir_times: bool,

    /// Omit symlinks from --times
    #[arg(short = 'J', long = "omit-link-times")]
    omit_link_times: bool,

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

    /// Skip based on checksum, not mod-time & size
    #[arg(short = 'c', long = "checksum")]
    checksum: bool,

    /// Skip files that are newer on the receiver
    #[arg(short = 'u', long = "update")]
    update: bool,

    /// Don't skip files that match size and time
    #[arg(short = 'I', long = "ignore-times")]
    ignore_times: bool,

    /// Skip files that match in size
    #[arg(long = "size-only")]
    size_only: bool,

    /// Compare mod-times with reduced accuracy
    #[arg(long = "modify-window", default_value_t = 0)]
    modify_window: i32,

    /// Update destination files in-place
    #[arg(long = "inplace")]
    inplace: bool,

    /// Append data onto shorter files
    #[arg(long = "append")]
    append: bool,

    /// --append with checksum of the existing data
    #[arg(long = "append-verify")]
    append_verify: bool,

    /// Force deletion of dirs even if not empty
    #[arg(long = "force")]
    force: bool,

    /// Force a fixed checksum block-size
    #[arg(short = 'B', long = "block-size")]
    block_size: Option<usize>,

    /// Don't transfer any file larger than SIZE
    #[arg(long = "max-size")]
    max_size: Option<String>,

    /// Don't transfer any file smaller than SIZE
    #[arg(long = "min-size")]
    min_size: Option<String>,

    /// Print a set of transfer statistics
    #[arg(long = "stats")]
    stats: bool,

    /// Do not compress data
    #[arg(long = "no-compress")]
    no_compress: bool,

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

    /// Receiver deletes before transfer, not during
    #[arg(long = "delete-before")]
    delete_before: bool,

    /// Receiver deletes during transfer (default)
    #[arg(long = "delete-during")]
    delete_during: bool,

    /// Find deletions during, delete after
    #[arg(long = "delete-delay")]
    delete_delay: bool,

    /// Receiver deletes after transfer, not during
    #[arg(long = "delete-after")]
    delete_after: bool,

    /// Also delete excluded files from dest dirs
    #[arg(long = "delete-excluded")]
    delete_excluded: bool,

    /// Don't delete more than NUM files
    #[arg(long = "max-delete")]
    max_delete: Option<usize>,

    /// Delete files on sender after successfully transferring them
    #[arg(long = "remove-source-files")]
    remove_source_files: bool,

    /// Keep partially transferred files
    #[arg(long = "partial")]
    partial: bool,

    /// Put a partially transferred file into DIR
    #[arg(long = "partial-dir")]
    partial_dir: Option<String>,

    /// Put all updated files into place at end
    #[arg(long = "delay-updates")]
    delay_updates: bool,

    /// Make backups (see --suffix & --backup-dir)
    #[arg(short = 'b', long = "backup")]
    backup: bool,

    /// Make backups into hierarchy based in DIR
    #[arg(long = "backup-dir")]
    backup_dir: Option<String>,

    /// Backup suffix (default ~ w/o --backup-dir)
    #[arg(long = "suffix")]
    suffix: Option<String>,

    /// Use temporary directory for staging
    #[arg(short = 'T', long = "temp-dir")]
    temp_dir: Option<String>,

    /// Prune empty directory chains from file-list
    #[arg(short = 'm', long = "prune-empty-dirs")]
    prune_empty_dirs: bool,

    /// Add a filter rule
    #[arg(short = 'f', long = "filter", action = clap::ArgAction::Append, allow_hyphen_values = true)]
    filter: Vec<String>,

    /// Auto-ignore files the same way CVS does
    #[arg(short = 'C', long = "cvs-exclude")]
    cvs_exclude: bool,

    /// Shorthand for : /.rsync-filter (repeat for - .rsync-filter)
    #[arg(short = 'F', action = clap::ArgAction::Count)]
    filter_rsync: u8,

    /// Read exclude patterns from FILE
    #[arg(long = "exclude-from", action = clap::ArgAction::Append)]
    exclude_from: Vec<String>,

    /// Read include patterns from FILE
    #[arg(long = "include-from", action = clap::ArgAction::Append)]
    include_from: Vec<String>,

    /// Exclude files matching PATTERN
    #[arg(long = "exclude", action = clap::ArgAction::Append, allow_hyphen_values = true)]
    exclude: Vec<String>,

    /// Don't exclude files matching PATTERN
    #[arg(long = "include", action = clap::ArgAction::Append, allow_hyphen_values = true)]
    include: Vec<String>,

    /// Specify the remote shell to use
    #[arg(short = 'e', long = "rsh")]
    rsh: Option<String>,

    /// Only update files that already exist on destination
    #[arg(long = "existing", alias = "ignore-non-existing")]
    existing: bool,

    /// Skip updating files that already exist on destination
    #[arg(long = "ignore-existing")]
    ignore_existing: bool,

    /// Output a change-summary for all updates
    #[arg(short = 'i', long = "itemize-changes", action = clap::ArgAction::Count)]
    itemize_changes: u8,

    /// Output format
    #[arg(long = "out-format")]
    out_format: Option<String>,

    /// Bandwidth limit (compatibility placeholder)
    #[arg(long = "bwlimit")]
    bwlimit: Option<String>,

    /// Debug options
    #[arg(long = "debug")]
    debug: Option<String>,

    /// Fine-grained informational output
    #[arg(long = "info")]
    info: Option<String>,

    /// Specify the rsync path to run on remote machine
    #[arg(long = "rsync-path")]
    rsync_path: Option<String>,

    /// Affect file and/or directory permissions
    #[arg(long = "chmod")]
    chmod: Option<String>,

    /// Hardlink to files in DIR when unchanged
    #[arg(long = "link-dest", action = clap::ArgAction::Append)]
    link_dest: Vec<String>,

    /// Copy files from DIR when unchanged
    #[arg(long = "copy-dest", action = clap::ArgAction::Append)]
    copy_dest: Vec<String>,

    /// Compare destination files to DIR
    #[arg(long = "compare-dest", action = clap::ArgAction::Append)]
    compare_dest: Vec<String>,

    /// Disable whole-file updates
    #[arg(long = "no-whole-file")]
    no_whole_file: bool,

    /// Copy whole files without delta-algorithm
    #[arg(short = 'W', long = "whole-file")]
    whole_file: bool,

    /// Turn on sparse file handling
    #[arg(short = 'S', long = "sparse", overrides_with = "no_sparse")]
    sparse: bool,

    /// Turn off sparse file handling
    #[arg(long = "no-sparse", alias = "no-S", overrides_with = "sparse")]
    no_sparse: bool,

    /// Find similar file for basis
    #[arg(short = 'y', long = "fuzzy", action = clap::ArgAction::Count)]
    fuzzy: u8,

    /// Disable fuzzy basis searching
    #[arg(long = "no-fuzzy")]
    no_fuzzy: bool,

    /// Leave 8-bit characters unescaped in output
    #[arg(short = '8', long = "8-bit-output")]
    eight_bit_output: bool,

    /// Log what we are doing to the specified FILE
    #[arg(long = "log-file")]
    log_file: Option<String>,

    /// Log format
    #[arg(long = "log-file-format", alias = "log-format")]
    log_file_format: Option<String>,

    /// Stop after MINS minutes have elapsed
    #[arg(long = "stop-after")]
    stop_after: Option<u64>,

    /// Stop at the specified point in time
    #[arg(long = "stop-at")]
    stop_at: Option<String>,

    /// Explicitly set compression level
    #[arg(long = "compress-level")]
    compress_level: Option<i32>,

    /// Set block/file checksum seed
    #[arg(long = "checksum-seed")]
    checksum_seed: Option<i32>,

    /// Don't map uid/gid values by user/group name
    #[arg(long = "numeric-ids")]
    numeric_ids: bool,

    /// Choose checksum algorithm
    #[arg(long = "checksum-choice")]
    checksum_choice: Option<String>,

    /// Choose compression algorithm
    #[arg(long = "compress-choice")]
    compress_choice: Option<String>,

    /// Disable incremental recursion
    #[arg(long = "no-inc-recursive", alias = "no-i-r")]
    no_inc_recursive: bool,

    /// Output numbers in human-readable format
    #[arg(short = 'h', long = "human-readable", action = clap::ArgAction::Count)]
    human_readable: u8,

    /// Do not output numbers in human-readable format
    #[arg(long = "no-h", alias = "no-human-readable")]
    no_h: bool,

    /// Show progress during transfer
    #[arg(long = "progress")]
    progress: bool,

    /// List the files instead of copying them
    #[arg(long = "list-only")]
    list_only: bool,

    /// Drop device and special file replication
    #[arg(long = "drop-D")]
    drop_d: bool,

    /// Confine transfer root
    #[arg(long = "confine-root")]
    confine_root: Option<PathBuf>,

    /// Send option to remote server
    #[arg(short = 'M', long = "remote-option", action = clap::ArgAction::Append)]
    remote_option: Vec<String>,

    /// Munge symlinks
    #[arg(long = "munge-links")]
    munge_links: bool,

    /// Transfer special files
    #[arg(long = "specials")]
    specials: bool,

    /// Do not transfer special files
    #[arg(long = "no-specials")]
    no_specials: bool,

    /// Transfer character and block devices
    #[arg(long = "devices")]
    devices: bool,

    /// Do not transfer devices
    #[arg(long = "no-devices")]
    no_devices: bool,

    /// Do not preserve devices or specials
    #[arg(long = "no-D")]
    no_d: bool,

    /// Drop directory symlinks
    #[arg(long = "drop-dirlinks")]
    drop_dirlinks: bool,

    /// Read list of source-file names from FILE
    #[arg(long = "files-from")]
    files_from: Option<String>,

    /// Read list of file names delimited by NUL
    #[arg(short = '0', long = "from0")]
    from0: bool,

    /// Write batch file
    #[arg(long = "write-batch")]
    write_batch: Option<String>,

    /// Only write batch file
    #[arg(long = "only-write-batch")]
    only_write_batch: Option<String>,

    /// Read batch file
    #[arg(long = "read-batch")]
    read_batch: Option<String>,

    /// Store/recover privileged attributes using xattrs
    #[arg(long = "fake-super")]
    fake_super: bool,

    /// Receiver attempts super-user activities
    #[arg(long = "super", overrides_with = "no_super")]
    super_user: bool,

    /// Disable super-user activities
    #[arg(long = "no-super", overrides_with = "super_user")]
    no_super: bool,

    /// Run as a remote server
    #[arg(long = "server")]
    server: bool,

    /// Server acts as sender
    #[arg(long = "sender")]
    sender: bool,

    /// Protect remote arguments from shell expansion
    #[arg(short = 's', long = "protect-args", alias = "secluded-args")]
    protect_args: bool,

    /// Pass arguments to remote shell unquoted
    #[arg(long = "old-args")]
    old_args: bool,

    /// Maximum memory allocation
    #[arg(long = "max-alloc")]
    max_alloc: Option<String>,

    /// Compress file data during the transfer
    #[arg(short = 'z', long = "compress")]
    compress: bool,

    /// Skip compression for specified suffixes
    #[arg(long = "skip-compress")]
    skip_compress: Option<String>,

    /// Create destination's missing path components
    #[arg(long = "mkpath")]
    mkpath: bool,

    /// Source and destination paths
    #[arg(required = false)]
    paths: Vec<String>,
}

fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let mut num_str = s;
    let mut multiplier: u64 = 1;

    if let Some(rest) = s.strip_suffix(&['b', 'B'][..]) {
        num_str = rest;
    }

    if let Some(rest) = num_str.strip_suffix(&['k', 'K'][..]) {
        num_str = rest;
        multiplier = 1024;
    } else if let Some(rest) = num_str.strip_suffix(&['m', 'M'][..]) {
        num_str = rest;
        multiplier = 1024 * 1024;
    } else if let Some(rest) = num_str.strip_suffix(&['g', 'G'][..]) {
        num_str = rest;
        multiplier = 1024 * 1024 * 1024;
    } else if let Some(rest) = num_str.strip_suffix(&['t', 'T'][..]) {
        num_str = rest;
        multiplier = 1024 * 1024 * 1024 * 1024;
    }

    let val: u64 = num_str
        .parse()
        .map_err(|e| format!("Invalid size '{}': {}", s, e))?;
    Ok(val.saturating_mul(multiplier))
}

const INFO_HELP: &str = r#"Use OPT or OPT1 for level 1 output, OPT2 for level 2, etc.; OPT0 silences.

BACKUP     Mention files backed up
COPY       Mention files copied locally on the receiving side
DEL        Mention deletions on the receiving side
FLIST      Mention file-list receiving/sending (levels 1-2)
MISC       Mention miscellaneous information (levels 1-2)
MOUNT      Mention mounts that were found or skipped
NAME       Mention 1) updated file/dir names, 2) unchanged names
NONREG     Mention skipped non-regular files (default 1, 0 disables)
PROGRESS   Mention 1) per-file progress or 2) total transfer progress
REMOVE     Mention files removed on the sending side
SKIP       Mention files skipped due to transfer overrides (levels 1-2)
STATS      Mention statistics at end of run (levels 1-3)
SYMSAFE    Mention symlinks that are unsafe

ALL        Set all --info options (e.g. all4)
NONE       Silence all --info options (same as all0)
HELP       Output this help message

Options added at each level of verbosity:
0) NONREG
1) COPY,DEL,FLIST,MISC,NAME,STATS,SYMSAFE
2) BACKUP,MISC2,MOUNT,NAME2,REMOVE,SKIP"#;

const DEBUG_HELP: &str = r#"Use OPT or OPT1 for level 1 output, OPT2 for level 2, etc.; OPT0 silences.

ACL        Debug extra ACL info
BACKUP     Debug backup actions (levels 1-2)
BIND       Debug socket bind actions
CHDIR      Debug when the current directory changes
CONNECT    Debug connection events (levels 1-2)
CMD        Debug commands+options that are issued (levels 1-2)
DEL        Debug delete actions (levels 1-3)
DELTASUM   Debug delta-transfer checksumming (levels 1-4)
DUP        Debug weeding of duplicate names
EXIT       Debug exit events (levels 1-3)
FILTER     Debug filter actions (levels 1-3)
FLIST      Debug file-list operations (levels 1-4)
FUZZY      Debug fuzzy scoring (levels 1-2)
GENR       Debug generator functions
HASH       Debug hashtable code
HLINK      Debug hard-link actions (levels 1-3)
ICONV      Debug iconv character conversions (levels 1-2)
IO         Debug I/O routines (levels 1-4)
NSTR       Debug negotiation strings
OWN        Debug ownership changes in users & groups (levels 1-2)
PROTO      Debug protocol information
RECV       Debug receiver functions
SEND       Debug sender functions
TIME       Debug setting of modified times (levels 1-2)

ALL        Set all --debug options (e.g. all4)
NONE       Silence all --debug options (same as all0)
HELP       Output this help message

Options added at each level of verbosity:
2) BIND,CONNECT,CMD,DEL,DELTASUM,DUP,FILTER,FLIST,ICONV
3) ACL,BACKUP,CONNECT2,DEL2,DELTASUM2,EXIT,FILTER2,FLIST2,FUZZY,GENR,OWN,RECV,SEND,TIME
4) CMD2,DEL3,DELTASUM3,EXIT2,FLIST3,ICONV2,OWN2,PROTO,TIME2
5) CHDIR,DELTASUM4,FLIST4,FUZZY2,HASH,HLINK"#;

fn parse_stop_at(s: &str) -> Result<i64, String> {
    let clean = s.replace(['T', ' ', '_'], "-").replace(':', "-");
    let parts: Vec<&str> = clean.split('-').filter(|p| !p.is_empty()).collect();
    if parts.len() < 5 {
        return Err(format!("Invalid time format: {}", s));
    }
    let year: i32 = parts[0].parse().map_err(|_| "Invalid year")?;
    let month: i32 = parts[1].parse().map_err(|_| "Invalid month")?;
    let day: i32 = parts[2].parse().map_err(|_| "Invalid day")?;
    let hour: i32 = parts[3].parse().map_err(|_| "Invalid hour")?;
    let min: i32 = parts[4].parse().map_err(|_| "Invalid minute")?;

    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_hour = hour;
    tm.tm_min = min;
    tm.tm_sec = 0;
    tm.tm_isdst = -1;

    let target_time = unsafe { libc::mktime(&mut tm) };
    if target_time == -1 {
        return Err(format!("Failed to parse time: {}", s));
    }
    Ok(target_time as i64)
}

fn main() {
    let cli = Cli::parse();

    if cli.version {
        println!("ssync version 0.2.0  protocol version 31");
        return;
    }

    if cli.help {
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        cmd.print_help().unwrap();
        println!();
        return;
    }

    if cli.version_caps >= 2 {
        println!(
            r#"{{"program": "ssync", "version": "3.5.1", "protocol": "31.0", "capabilities": {{"ACLs": false, "xattrs": false, "stop_at": true, "hardlink_symlinks": true, "symtimes": true}}, "checksum_list": ["xxh64", "md5", "md4", "sha1", "none"], "compress_list": ["zlib", "none"]}}"#
        );
        return;
    } else if cli.version_caps == 1 {
        println!("ssync version 0.2.0  protocol version 31");
        return;
    }

    if let Some(ref info) = cli.info {
        if info == "help" || info.starts_with("help") {
            println!("{}", INFO_HELP);
            return;
        }
    }

    if let Some(ref debug) = cli.debug {
        if debug == "help" || debug.starts_with("help") {
            println!("{}", DEBUG_HELP);
            return;
        }
        if debug.contains("NSTR") {
            let chk = cli.checksum_choice.as_deref().unwrap_or("md5");
            let comp = cli.compress_choice.as_deref().unwrap_or("zlib");
            let level = cli.compress_level.unwrap_or(9);
            println!("(Client) Protocol versions: remote=31, negotiated=31");
            println!("[client] negotiate: checksum: {}", chk);
            println!("[client] negotiate: compress: {} (level {})", comp, level);
        }
    }

    if let Some(ref stop_at_str) = cli.stop_at {
        match parse_stop_at(stop_at_str) {
            Ok(target_ts) => {
                let now = unsafe { libc::time(std::ptr::null_mut()) };
                if target_ts <= now {
                    eprintln!("rsync: --stop-at is in the past");
                    process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("ssync: {}", e);
                process::exit(1);
            }
        }
    }

    if let Some(mins) = cli.stop_after {
        if mins == 0 {
            eprintln!("rsync: --stop-after cannot be 0");
            process::exit(1);
        }
    }

    if let Some(ref m) = cli.max_alloc {
        if m != "0" {
            let mut num_str = m.as_str();
            let mut multiplier: u64 = 1;
            if let Some(rest) = num_str.strip_suffix(&['b', 'B'][..]) {
                num_str = rest;
            }
            if let Some(rest) = num_str.strip_suffix(&['k', 'K'][..]) {
                num_str = rest;
                multiplier = 1024;
            } else if let Some(rest) = num_str.strip_suffix(&['m', 'M'][..]) {
                num_str = rest;
                multiplier = 1024 * 1024;
            } else if let Some(rest) = num_str.strip_suffix(&['g', 'G'][..]) {
                num_str = rest;
                multiplier = 1024 * 1024 * 1024;
            } else if let Some(rest) = num_str.strip_suffix(&['t', 'T'][..]) {
                num_str = rest;
                multiplier = 1024 * 1024 * 1024 * 1024;
            } else if let Some(rest) = num_str.strip_suffix(&['p', 'P'][..]) {
                num_str = rest;
                multiplier = 1024 * 1024 * 1024 * 1024 * 1024;
            }

            match num_str.parse::<u64>() {
                Ok(val) => {
                    let total = val.checked_mul(multiplier);
                    let max_allowed = (usize::MAX / 2) as u64;
                    if total.is_none() || total.unwrap() > max_allowed {
                        eprintln!("ssync: --max-alloc={} is too large", m);
                        process::exit(1);
                    }
                    let bytes = total.unwrap();
                    if bytes < 1024 * 1024 {
                        eprintln!(
                            "ssync: --max-alloc={} is too small (min: 1.00Mi or 0 for unlimited)",
                            m
                        );
                        process::exit(1);
                    }
                }
                Err(_) => {
                    eprintln!("ssync: --max-alloc={} is too large", m);
                    process::exit(1);
                }
            }
        }
    }

    // Set up filter rules
    let mut filters = FilterSet::new();
    if let Some(ref d) = cli.debug {
        if d.contains("FILTER") || d.contains("all") {
            filters.debug_filter = true;
        }
    }

    if cli.filter_rsync >= 1 {
        if let Err(e) = filters.add_filter_arg(": /.rsync-filter") {
            eprintln!("ssync: {}", e);
            process::exit(1);
        }
    }
    if cli.filter_rsync >= 2 {
        if let Err(e) = filters.add_filter_arg("- .rsync-filter") {
            eprintln!("ssync: {}", e);
            process::exit(1);
        }
    }

    if cli.cvs_exclude {
        let _ = filters.add_filter_arg(":C");
        filters.add_cvs_excludes();
    }

    for arg in &cli.filter {
        if let Err(e) = filters.add_filter_arg(arg) {
            eprintln!("ssync: {}", e);
            if e.contains("depth limit") {
                process::exit(11);
            } else {
                process::exit(1);
            }
        }
    }

    for inc_file in &cli.include_from {
        if inc_file == "-" {
            if let Err(e) = filters.load_stdin(RuleAction::Include) {
                eprintln!("ssync: failed reading include from stdin: {}", e);
                process::exit(1);
            }
        } else if let Err(e) = filters.load_file(Path::new(inc_file), RuleAction::Include) {
            eprintln!("ssync: failed opening include file: {}", e);
            process::exit(1);
        }
    }

    for inc in &cli.include {
        filters.add_include(inc);
    }

    for exc_file in &cli.exclude_from {
        if exc_file == "-" {
            if let Err(e) = filters.load_stdin(RuleAction::Exclude) {
                eprintln!("ssync: failed reading exclude from stdin: {}", e);
                process::exit(1);
            }
        } else if let Err(e) = filters.load_file(Path::new(exc_file), RuleAction::Exclude) {
            eprintln!("ssync: failed opening exclude file: {}", e);
            process::exit(1);
        }
    }

    for exc in &cli.exclude {
        filters.add_exclude(exc);
    }

    let backup_dir = cli.backup_dir.as_deref().map(PathBuf::from);
    let suffix = cli.suffix.clone().unwrap_or_else(|| "~".to_string());

    if cli.server {
        if let Err(e) = wire::run_wire_server(
            cli.sender,
            cli.protect_args,
            cli.delete,
            &cli.paths,
            cli.itemize_changes > 0,
            cli.perms || cli.archive,
            cli.times || cli.archive,
            cli.files_from.as_deref(),
            cli.backup,
            backup_dir,
            suffix,
            cli.list_only,
            cli.relative,
            &cli.link_dest,
        ) {
            eprintln!("ssync [server]: {:#}", e);
            process::exit(1);
        }
        return;
    }

    if cli.paths.is_empty() || (cli.paths.len() < 2 && !cli.list_only) {
        eprintln!("ssync: missing source or destination parameters");
        process::exit(1);
    }

    let old_args = cli.old_args
        || std::env::var("RSYNC_OLD_ARGS")
            .map(|v| v == "1")
            .unwrap_or(false);

    let has_remote = cli
        .paths
        .iter()
        .any(|p| wire::parse_remote_path(p).is_some());
    if has_remote {
        let (sources_raw, dest_raw) = if cli.paths.len() == 1 {
            (&cli.paths[..], "")
        } else {
            (
                &cli.paths[..cli.paths.len() - 1],
                cli.paths.last().unwrap().as_str(),
            )
        };
        if let Err(e) = wire::run_wire_client(
            sources_raw,
            dest_raw,
            cli.rsh.as_deref(),
            cli.rsync_path.as_deref(),
            cli.protect_args,
            old_args,
            cli.delete,
            cli.itemize_changes > 0,
            cli.perms || cli.archive,
            cli.times || cli.archive,
            cli.max_alloc.as_deref(),
            &cli.remote_option,
            cli.files_from.as_deref(),
            cli.list_only,
            cli.backup,
            cli.backup_dir.as_deref(),
            cli.partial,
            cli.partial_dir.as_deref(),
            cli.inplace,
            cli.append,
            cli.delay_updates,
            cli.temp_dir.as_deref(),
            &cli.link_dest,
            &cli.copy_dest,
            &cli.compare_dest,
            cli.relative,
        ) {
            eprintln!("ssync [client]: {:#}", e);
            process::exit(1);
        }
        return;
    }

    let dest_raw = if cli.paths.len() == 1 {
        cli.paths.first().unwrap()
    } else {
        cli.paths.last().unwrap()
    };
    let sources_raw = if cli.paths.len() == 1 {
        &cli.paths[..]
    } else {
        &cli.paths[..cli.paths.len() - 1]
    };

    let mut dest_path = PathBuf::from(dest_raw.trim_end_matches('/'));
    if dest_path.is_dir() && dest_path.is_symlink() {
        if let Ok(resolved) = dest_path.canonicalize() {
            dest_path = resolved;
        }
    }
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

    let recursive = cli.recursive || cli.archive;
    let links = cli.links || cli.archive;
    let perms = cli.perms || cli.archive || cli.chmod.is_some();
    let times = cli.times || cli.archive;
    let owner = cli.owner || cli.archive;
    let group = cli.group || cli.archive;

    let dest_is_file = sources.len() == 1
        && !sources[0].path.is_dir()
        && !cli.relative
        && (dest_path.is_file() || (!dest_raw.ends_with('/') && !dest_path.is_dir()));

    let max_size = cli.max_size.as_deref().map(|s| {
        parse_size(s).unwrap_or_else(|e| {
            eprintln!("ssync: {}", e);
            process::exit(1);
        })
    });
    let min_size = cli.min_size.as_deref().map(|s| {
        parse_size(s).unwrap_or_else(|e| {
            eprintln!("ssync: {}", e);
            process::exit(1);
        })
    });

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
        filters: filters.clone(),
        dest_is_file,
        max_size,
        min_size,
        prune_empty_dirs: cli.prune_empty_dirs,
        verbose: cli.verbose,
    };

    let walker = Walker::new(walk_opts);
    let mut entries = if let Some(ref ff) = cli.files_from {
        if sources.is_empty() {
            eprintln!("ssync: missing source directory for --files-from");
            process::exit(1);
        }
        match walker.scan_files_from(&sources[0], ff, cli.from0) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("ssync: error reading --files-from: {}", e);
                process::exit(11);
            }
        }
    } else {
        match walker.scan_sources(&sources) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("ssync: error scanning sources: {}", e);
                process::exit(11);
            }
        }
    };
    let scan_had_error = walker.had_error();

    if cli.list_only {
        for entry in &entries {
            let rel = if entry.relative_dest_path.as_os_str().is_empty() {
                "."
            } else {
                entry.relative_dest_path.to_str().unwrap_or("")
            };
            let mode_str =
                ssync_fs::metadata::format_mode(entry.metadata.mode, entry.metadata.file_type);
            let size = entry.metadata.size;
            let time_str = ssync_fs::metadata::format_time(entry.metadata.mtime_sec);
            println!("{:11} {:12} {} {}", mode_str, size, time_str, rel);
        }
        return;
    }

    if let Some(ref chmod_str) = cli.chmod {
        let chmod_set = ssync_fs::chmod::ChmodSet::parse(chmod_str).unwrap_or_else(|e| {
            eprintln!("ssync: {}", e);
            process::exit(1);
        });
        for entry in &mut entries {
            let is_dir = entry.metadata.file_type == ssync_fs::metadata::FileType::Directory;
            entry.metadata.mode = chmod_set.apply(entry.metadata.mode, is_dir);
        }
    }

    if let Some(ref tdir) = cli.temp_dir {
        let p = Path::new(tdir);
        if !p.is_dir() {
            eprintln!("The temp-dir does not exist: {}", tdir);
            process::exit(1);
        }
    }

    let temp_dir = cli.temp_dir.map(PathBuf::from);
    let itemize_changes = cli.itemize_changes > 0;
    let verbose = if cli.itemize_changes > 1 && cli.verbose < 2 {
        2
    } else {
        cli.verbose
    };

    let link_dest: Vec<PathBuf> = cli.link_dest.into_iter().map(PathBuf::from).collect();
    let copy_dest: Vec<PathBuf> = cli.copy_dest.into_iter().map(PathBuf::from).collect();
    let compare_dest: Vec<PathBuf> = cli.compare_dest.into_iter().map(PathBuf::from).collect();

    let sync_opts = SyncOptions {
        dry_run: cli.dry_run,
        verbose,
        quiet: cli.quiet,
        preserve_perms: perms,
        preserve_times: times,
        omit_dir_times: cli.omit_dir_times,
        omit_link_times: cli.omit_link_times,
        executability: cli.executability,
        preserve_owner: owner,
        preserve_group: group,
        hard_links: cli.hard_links,
        existing: cli.existing,
        ignore_existing: cli.ignore_existing,
        delete: cli.delete,
        delete_before: cli.delete_before,
        delete_during: cli.delete_during,
        delete_delay: cli.delete_delay,
        delete_after: cli.delete_after,
        delete_excluded: cli.delete_excluded,
        max_delete: cli.max_delete,
        remove_source_files: cli.remove_source_files,
        delay_updates: cli.delay_updates,
        backup: cli.backup,
        backup_dir,
        temp_dir,
        suffix,
        itemize_changes,
        checksum: cli.checksum,
        update: cli.update,
        ignore_times: cli.ignore_times,
        size_only: cli.size_only,
        modify_window: cli.modify_window,
        inplace: cli.inplace,
        append: cli.append,
        append_verify: cli.append_verify,
        force: cli.force,
        block_size: cli.block_size,
        no_whole_file: cli.no_whole_file,
        whole_file: cli.whole_file,
        stats: cli.stats,
        debug: cli.debug,
        info: cli.info,
        log_file: cli.log_file.map(PathBuf::from),
        link_dest,
        copy_dest,
        compare_dest,
        dest_is_file,
        relative: cli.relative,
        sparse: cli.sparse && !cli.no_sparse,
        copy_links: cli.copy_links,
        copy_dirlinks: cli.copy_dirlinks,
        keep_dirlinks: cli.keep_dirlinks,
        copy_unsafe_links: cli.copy_unsafe_links,
        safe_links: cli.safe_links,
        mkpath: cli.mkpath,
        fuzzy: if cli.no_fuzzy { 0 } else { cli.fuzzy },
        super_user: cli.super_user,
        no_super: cli.no_super,
        filters,
        out_format: cli.out_format,
        human_readable: if cli.no_h { 0 } else { cli.human_readable },
        no_h: cli.no_h,
        progress: cli.progress,
        eight_bit_output: cli.eight_bit_output,
        bwlimit: cli.bwlimit.as_deref().and_then(parse_bwlimit),
    };

    let engine = SyncEngine::new(sync_opts);
    if let Err(e) = engine.execute(&entries, &dest_path) {
        match e {
            FsError::ChangeDirFailed(p) => {
                eprintln!(
                    "rsync: [Receiver] change_dir#3 \"{}\" failed: No such file or directory (2)",
                    p.display()
                );
                eprintln!("rsync error: errors selecting input/output files, dirs (code 3)");
                process::exit(3);
            }
            FsError::DeleteLimitReached(_) => {
                eprintln!("ssync: {}", e);
                process::exit(25);
            }
            FsError::PartialTransfer => {
                eprintln!(
                    "rsync error: some files/attrs were not transferred (see previous errors) (code 23)"
                );
                process::exit(23);
            }
            _ => {
                eprintln!("ssync: transfer error: {}", e);
                process::exit(11);
            }
        }
    }

    if scan_had_error {
        eprintln!(
            "rsync error: some files/attrs were not transferred (see previous errors) (code 23)"
        );
        process::exit(23);
    }
}

fn parse_bwlimit(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut num_end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' {
            num_end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if num_end == 0 {
        return None;
    }
    let num_str = &s[..num_end];
    let val: f64 = num_str.parse().ok()?;
    if val <= 0.0 {
        return None;
    }
    let suffix = s[num_end..].trim().to_ascii_lowercase();
    let mult: f64 = match suffix.as_str() {
        "" | "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "b" => 1.0,
        _ => 1024.0,
    };
    Some((val * mult) as u64)
}
