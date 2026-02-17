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

fn main() {
    reset_sigpipe();

    let cli = cli::Cli::parse();
    let repo_root = resolve_repo_root(cli.path.as_deref());
    let (git_dir, common_dir) = status::resolve_git_dirs(&repo_root);

    let state_dir = default_state_dir();
    fs::create_dir_all(&state_dir).expect("git-status-watch: cannot create state dir");
    let state_path = state_file_path(&state_dir, &repo_root);

    if cli.once {
        // Fast path: if a watcher is maintaining the state file, just read it
        if is_watched(&state_path) {
            if let Some(status) = read_state_file(&state_path) {
                let output = format_output(&status, cli.format.as_deref());
                let _ = print_stdout(&output);
                return;
            }
        }
        let status = status::compute_status(&repo_root, &git_dir, &common_dir);
        let output = format_output(&status, cli.format.as_deref());
        write_state_file(&state_path, &status);
        let _ = print_stdout(&output);
        return;
    }

    // Watch mode: coordinate via lock file
    let _lock = try_lock(&state_path);

    if _lock.is_none() {
        run_follower(&state_path, cli.format.as_deref(), cli.always_print);
    } else {
        run_leader(&repo_root, &git_dir, &common_dir, &state_path, &cli);
    }
}

fn run_leader(
    repo_root: &Path,
    git_dir: &Path,
    common_dir: &Path,
    state_path: &Path,
    cli: &cli::Cli,
) {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let status = status::compute_status(repo_root, git_dir, common_dir);
    let output = format_output(&status, cli.format.as_deref());
    write_state_file(state_path, &status);
    if write_line(&mut out, &output).is_err() {
        return;
    }
    let mut last_status = status;

    let (rx, _debouncer) = watcher::start_watcher(repo_root, cli.debounce_ms);

    loop {
        match rx.recv() {
            Ok(watcher::WatchEvent::Changed) => {
                let status = status::compute_status(repo_root, git_dir, common_dir);
                if cli.always_print || status != last_status {
                    let output = format_output(&status, cli.format.as_deref());
                    write_state_file(state_path, &status);
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

fn run_follower(state_path: &Path, template: Option<&str>, always_print: bool) {
    use std::sync::mpsc;
    use std::time::Duration;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut last_status: Option<types::GitStatus> = None;

    if let Some(status) = read_state_file(state_path) {
        let output = format_output(&status, template);
        if write_line(&mut out, &output).is_err() {
            return;
        }
        last_status = Some(status);
    }

    let (tx, rx) = mpsc::channel();
    let state_dir = state_path.parent().expect("state path has no parent");
    let state_name = state_path
        .file_name()
        .expect("state path has no filename")
        .to_os_string();

    let name = state_name.clone();
    let mut debouncer = notify_debouncer_mini::new_debouncer(
        Duration::from_millis(50),
        move |result: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            if let Ok(events) = result {
                if events
                    .iter()
                    .any(|e| e.path.file_name().is_some_and(|f| f == name))
                {
                    let _ = tx.send(());
                }
            }
        },
    )
    .expect("failed to create file watcher");

    debouncer
        .watcher()
        .watch(state_dir, notify::RecursiveMode::NonRecursive)
        .expect("failed to watch state directory");

    loop {
        match rx.recv() {
            Ok(()) => {
                if let Some(status) = read_state_file(state_path) {
                    if always_print || last_status.as_ref() != Some(&status) {
                        let output = format_output(&status, template);
                        if write_line(&mut out, &output).is_err() {
                            return;
                        }
                        last_status = Some(status);
                    }
                }
            }
            Err(_) => return,
        }
    }
}

fn default_state_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("git-status-watch")
}

fn resolve_repo_root(path: Option<&Path>) -> PathBuf {
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
    let json = serde_json::to_string(status).unwrap();
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, json.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn read_state_file(path: &Path) -> Option<types::GitStatus> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Check if another watcher holds the lock for this state file.
fn is_watched(state_path: &Path) -> bool {
    try_lock(state_path).is_none()
}

fn format_output(status: &types::GitStatus, template: Option<&str>) -> String {
    match template {
        Some(t) => format::format_custom(status, t),
        None => format::format_json(status),
    }
}

fn print_stdout(s: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{}", s)?;
    out.flush()
}

fn write_line(out: &mut io::StdoutLock, s: &str) -> io::Result<()> {
    writeln!(out, "{}", s)?;
    out.flush()
}

#[cfg(unix)]
fn try_lock(state_path: &Path) -> Option<fs::File> {
    use std::os::unix::io::AsRawFd;
    let lock_path = state_path.with_extension("lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&lock_path)
        .ok()?;
    let fd = file.as_raw_fd();
    if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        Some(file)
    } else {
        None
    }
}

#[cfg(not(unix))]
fn try_lock(_state_path: &Path) -> Option<fs::File> {
    None
}

#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}
