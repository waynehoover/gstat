mod cli;
mod format;
mod status;
mod types;
mod watcher;

use clap::Parser;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;
use std::sync::mpsc;
use std::time::Duration;

fn main() {
    reset_sigpipe();

    let cli = cli::Cli::parse();
    let repo_root = resolve_repo_root(cli.path.as_deref());

    // Print initial status
    let status = status::compute_status(&repo_root);
    let output = format_output(&status, cli.format.as_deref());
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
                    if print_line(&output).is_err() {
                        return;
                    }
                    last_output = Some(output);
                }
            }
            Ok(watcher::WatchEvent::Error(e)) => {
                eprintln!("gstat: watcher error: {}", e);
            }
            Err(_) => {
                eprintln!("gstat: watcher channel closed");
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
        eprintln!("gstat: not a git repository: {}", dir.display());
        process::exit(1);
    }

    PathBuf::from(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string(),
    )
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
