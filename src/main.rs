mod cli;
mod format;
mod status;
mod types;
mod watcher;

use clap::Parser;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc;
use std::time::Duration;

fn main() {
    reset_sigpipe();

    let cli = cli::Cli::parse();
    let repo_root = resolve_repo_root(cli.path.as_deref());

    let state_file = cli.state_dir.as_ref().map(|dir| {
        fs::create_dir_all(dir).expect("git-status-watch: cannot create state dir");
        state_file_path(dir, &repo_root)
    });

    // Print initial status
    let status = status::compute_status(&repo_root);
    let output = format_output(&status, cli.format.as_deref());
    if let Some(ref sf) = state_file {
        write_state_file(sf, &status);
    }
    if print_line(&output).is_err() {
        return;
    }
    let mut last_output = Some(output);

    if cli.once {
        return;
    }

    // Start filesystem watcher
    let (rx, _debouncer) = watcher::start_watcher(&repo_root, cli.debounce_ms);

    // Drain any events triggered by the initial git status command
    drain(&rx);

    loop {
        match rx.recv() {
            Ok(watcher::WatchEvent::Changed) => {
                let status = status::compute_status(&repo_root);
                let output = format_output(&status, cli.format.as_deref());

                // Drain feedback events caused by git status touching .git/
                drain(&rx);

                if cli.always_print || last_output.as_ref() != Some(&output) {
                    if let Some(ref sf) = state_file {
                        write_state_file(sf, &status);
                    }
                    if print_line(&output).is_err() {
                        return;
                    }
                    last_output = Some(output);
                }
            }
            Ok(watcher::WatchEvent::Error(e)) => {
                eprintln!("git-status-watch: watcher error: {}", e);
            }
            Err(_) => {
                eprintln!("git-status-watch: watcher channel closed");
                process::exit(1);
            }
        }
    }
}

/// Drain any pending events from the channel, waiting briefly for feedback to settle.
fn drain(rx: &mpsc::Receiver<watcher::WatchEvent>) {
    // Give git's file writes time to trigger and be debounced
    std::thread::sleep(Duration::from_millis(150));
    while rx.try_recv().is_ok() {}
}

fn resolve_repo_root(path: Option<&std::path::Path>) -> PathBuf {
    let dir = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().expect("cannot determine current directory"));

    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&dir)
        .output()
        .expect("failed to run git");

    if !output.status.success() {
        eprintln!("git-status-watch: not a git repository: {}", dir.display());
        process::exit(1);
    }

    PathBuf::from(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string(),
    )
}

/// Compute the state file path for a repo: `<state_dir>/<encoded_repo_path>`
fn state_file_path(state_dir: &Path, repo_root: &Path) -> PathBuf {
    let encoded = repo_root
        .to_string_lossy()
        .replace('/', "%2F");
    state_dir.join(encoded)
}

/// Atomically write status to the state file (write tmp + rename).
/// Always writes tab-separated fields regardless of --format.
fn write_state_file(path: &Path, status: &types::GitStatus) {
    let content = format::format_custom(
        status,
        "{branch}\\t{staged}\\t{modified}\\t{untracked}\\t{conflicted}\\t{ahead}\\t{behind}\\t{stash}\\t{state}",
    );
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, &content).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn format_output(status: &types::GitStatus, template: Option<&str>) -> String {
    match template {
        Some(t) => format::format_custom(status, t),
        None => format::format_json(status),
    }
}

fn print_line(output: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", output)?;
    handle.flush()
}

#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}
