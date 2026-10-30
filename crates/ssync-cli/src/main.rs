//! ssync: Clean-room, high-performance file synchronization tool in Rust.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "ssync",
    author = "ssync contributors",
    version,
    about = "Fast, safe, flag-compatible file synchronization tool in Rust"
)]
struct Cli {
    /// Recurse into directories
    #[arg(short = 'r', long = "recursive")]
    recursive: bool,

    /// Archive mode; equals -rlptgoD
    #[arg(short = 'a', long = "archive")]
    archive: bool,

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

    /// Source and destination paths
    #[arg(required = false)]
    paths: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    if cli.version_caps >= 2 {
        // Output JSON capabilities when -VV is requested (used by rsyncfns in tests)
        println!(r#"{{"ACLs": false, "xattrs": false, "sha256": true, "sha512": true}}"#);
        return;
    }

    if cli.paths.is_empty() {
        eprintln!("ssync: missing source/destination parameters");
        std::process::exit(1);
    }

    if cli.verbose > 0 {
        println!("ssync: starting synchronization (dry-run={})", cli.dry_run);
    }
}
