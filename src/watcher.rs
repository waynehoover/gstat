use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub enum WatchEvent {
    Changed,
    Error(String),
}

pub fn start_watcher(
    repo_root: &Path,
    debounce_ms: u64,
) -> (mpsc::Receiver<WatchEvent>, notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>) {
    let (tx, rx) = mpsc::channel();
    let repo_root_buf = repo_root.to_path_buf();

    let mut debouncer = new_debouncer(
        Duration::from_millis(debounce_ms),
        move |result: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            match result {
                Ok(events) => {
                    let dominated_events = events
                        .iter()
                        .any(|e| e.kind == DebouncedEventKind::Any && is_relevant(&e.path, &repo_root_buf));
                    if dominated_events {
                        let _ = tx.send(WatchEvent::Changed);
                    }
                }
                Err(e) => {
                    let _ = tx.send(WatchEvent::Error(e.to_string()));
                }
            }
        },
    )
    .expect("failed to create file watcher");

    use notify::RecursiveMode;
    debouncer
        .watcher()
        .watch(repo_root, RecursiveMode::Recursive)
        .expect("failed to watch repository");

    (rx, debouncer)
}

fn is_relevant(path: &PathBuf, repo_root: &Path) -> bool {
    let relative = match path.strip_prefix(repo_root) {
        Ok(r) => r,
        Err(_) => return true,
    };

    let components: Vec<_> = relative.components().collect();
    if components.is_empty() {
        return true;
    }

    let first = components[0].as_os_str().to_string_lossy();
    if first == ".git" && components.len() > 1 {
        let second = components[1].as_os_str().to_string_lossy();
        if second == "objects" || second == "logs" {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_git_objects() {
        let root = PathBuf::from("/repo");
        assert!(!is_relevant(
            &PathBuf::from("/repo/.git/objects/pack/something"),
            &root
        ));
        assert!(!is_relevant(
            &PathBuf::from("/repo/.git/logs/HEAD"),
            &root
        ));
    }

    #[test]
    fn allow_git_index() {
        let root = PathBuf::from("/repo");
        assert!(is_relevant(&PathBuf::from("/repo/.git/index"), &root));
        assert!(is_relevant(&PathBuf::from("/repo/.git/HEAD"), &root));
        assert!(is_relevant(
            &PathBuf::from("/repo/.git/refs/heads/main"),
            &root
        ));
    }

    #[test]
    fn allow_worktree_files() {
        let root = PathBuf::from("/repo");
        assert!(is_relevant(&PathBuf::from("/repo/src/main.rs"), &root));
        assert!(is_relevant(&PathBuf::from("/repo/Cargo.toml"), &root));
    }
}
