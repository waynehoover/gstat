use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "git-status-watch", about = "Reactive git status watcher")]
pub struct Cli {
    /// Path to the git repository (defaults to current directory)
    pub path: Option<PathBuf>,

    /// Custom format string (e.g. '{branch} +{staged} ~{modified}')
    #[arg(long)]
    pub format: Option<String>,

    /// Print status once and exit
    #[arg(long)]
    pub once: bool,

    /// Debounce window in milliseconds
    #[arg(long, default_value = "75")]
    pub debounce_ms: u64,

    /// Print on every event even if status unchanged
    #[arg(long)]
    pub always_print: bool,
}
