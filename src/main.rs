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
    let (git_dir, common_dir) = status::resolve_git_dirs(&repo_root);

    let state_file = cli.state_dir.as_ref().map(|dir| {
        fs::create_dir_all(dir).expect("git-status-watch: cannot create state dir");
        state_file_path(dir, &repo_root)
    });

    let stdout = io::stdout();
    let mut out = stdout.lock();

    let status = status::compute_status(&repo_root, &git_dir, &common_dir);
    let output = format_output(&status, cli.format.as_deref());
    if let Some(ref sf) = state_file {
        write_state_file(sf, &status);
    }
    if write_line(&mut out, &output).is_err() {
        return;
    }
    let mut last_status = status;

    if cli.once {
        return;
    }

    let (rx, _debouncer) = watcher::start_watcher(&repo_root, cli.debounce_ms);

    drain(&rx);

    loop {
        match rx.recv() {
            Ok(watcher::WatchEvent::Changed) => {
                let status = status::compute_status(&repo_root, &git_dir, &common_dir);

                drain(&rx);

                if cli.always_print || status != last_status {
                    let output = format_output(&status, cli.format.as_deref());
                    if let Some(ref sf) = state_file {
                        write_state_file(sf, &status);
                    }
                    if write_line(&mut out, &output).is_err() {
                        return;
                    }
                    last_status = status;
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

fn drain(rx: &mpsc::Receiver<watcher::WatchEvent>) {
    std::thread::sleep(Duration::from_millis(150));
    while rx.try_recv().is_ok() {}
}

fn resolve_repo_root(path: Option<&std::path::Path>) -> PathBuf {
    let dir = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().expect("cannot determine current directory"));

    let output = process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&dir)
        .stderr(process::Stdio::null())
        .output()
        .expect("failed to run git");

    if !output.status.success() {
        eprintln!(
            "git-status-watch: not a git repository: {}",
            dir.display()
        );
        process::exit(1);
    }

    let s = String::from_utf8(output.stdout)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    PathBuf::from(s.trim())
}

fn state_file_path(state_dir: &Path, repo_root: &Path) -> PathBuf {
    let encoded = repo_root.to_string_lossy().replace('/', "%2F");
    state_dir.join(encoded)
}

fn write_state_file(path: &Path, status: &types::GitStatus) {
    use std::fmt::Write as FmtWrite;
    let mut content = String::with_capacity(128);
    let _ = write!(
        content,
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        status.branch,
        status.staged,
        status.modified,
        status.untracked,
        status.conflicted,
        status.ahead,
        status.behind,
        status.stash,
        status.state
    );
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, content.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn format_output(status: &types::GitStatus, template: Option<&str>) -> String {
    match template {
        Some(t) => format::format_custom(status, t),
        None => format::format_json(status),
    }
}

fn write_line(out: &mut io::StdoutLock, s: &str) -> io::Result<()> {
    writeln!(out, "{}", s)?;
    out.flush()
}

#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}
