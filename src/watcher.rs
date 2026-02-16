use notify_debouncer_mini::new_debouncer;
use std::path::Path;
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
                        .any(|e| is_relevant(&e.path, &repo_root_buf));
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

fn is_relevant(path: &Path, repo_root: &Path) -> bool {
    let relative = match path.strip_prefix(repo_root) {
        Ok(r) => r,
        Err(_) => return true,
    };

    let mut components = relative.components();
    let first = match components.next() {
        Some(c) => c,
        None => return true,
    };

    if first.as_os_str() != ".git" {
        return true;
    }

    match components.next() {
        None => true,
        Some(second) => {
            let s = second.as_os_str();
            s == "HEAD"
                || s == "index"
                || s == "refs"
                || s == "MERGE_HEAD"
                || s == "REBASE_HEAD"
                || s == "CHERRY_PICK_HEAD"
                || s == "REVERT_HEAD"
                || s == "BISECT_LOG"
                || s == "rebase-merge"
                || s == "rebase-apply"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn filter_git_noise() {
        let root = PathBuf::from("/repo");
        assert!(!is_relevant(
            &PathBuf::from("/repo/.git/COMMIT_EDITMSG"),
            &root
        ));
        assert!(!is_relevant(
            &PathBuf::from("/repo/.git/index.lock"),
            &root
        ));
        assert!(!is_relevant(
            &PathBuf::from("/repo/.git/config"),
            &root
        ));
    }

    #[test]
    fn allow_git_state_files() {
        let root = PathBuf::from("/repo");
        assert!(is_relevant(&PathBuf::from("/repo/.git/index"), &root));
        assert!(is_relevant(&PathBuf::from("/repo/.git/HEAD"), &root));
        assert!(is_relevant(
            &PathBuf::from("/repo/.git/refs/heads/main"),
            &root
        ));
        assert!(is_relevant(
            &PathBuf::from("/repo/.git/MERGE_HEAD"),
            &root
        ));
        assert!(is_relevant(
            &PathBuf::from("/repo/.git/rebase-merge/done"),
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
