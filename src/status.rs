use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::types::{GitStatus, OperationState};

/// Resolve the worktree-aware git directory and common directory.
/// For normal repos both are `repo_root/.git`.
/// For worktrees, git_dir is worktree-specific and common_dir is shared.
pub fn resolve_git_dirs(repo_root: &Path) -> (PathBuf, PathBuf) {
    let dot_git = repo_root.join(".git");
    if !dot_git.is_file() {
        return (dot_git.clone(), dot_git);
    }
    if let Ok(content) = std::fs::read_to_string(&dot_git) {
        if let Some(dir) = content.strip_prefix("gitdir: ") {
            let dir = dir.trim();
            let git_dir = if Path::new(dir).is_absolute() {
                PathBuf::from(dir)
            } else {
                repo_root.join(dir)
            };
            if let Ok(common) = std::fs::read_to_string(git_dir.join("commondir")) {
                let common = common.trim();
                let common_dir = if Path::new(common).is_absolute() {
                    PathBuf::from(common)
                } else {
                    git_dir.join(common)
                };
                return (git_dir, common_dir);
            }
            return (git_dir.clone(), git_dir);
        }
    }
    (dot_git.clone(), dot_git)
}

pub fn compute_status(repo_root: &Path, git_dir: &Path, common_dir: &Path) -> GitStatus {
    let porcelain = run_git(repo_root, &[
        "-c",
        "gc.auto=0",
        "--no-optional-locks",
        "status",
        "--porcelain=v2",
        "--branch",
    ]);
    let (branch, ahead, behind, staged, modified, untracked, conflicted) =
        parse_porcelain_v2(&porcelain);

    let stash = stash_count(common_dir);
    let state = detect_operation_state(git_dir);

    GitStatus {
        branch,
        staged,
        modified,
        untracked,
        conflicted,
        ahead,
        behind,
        stash,
        state,
    }
}

fn run_git(repo_root: &Path, args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .stderr(Stdio::null())
        .output()
        .map(|o| {
            String::from_utf8(o.stdout)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
        })
        .unwrap_or_default()
}

fn parse_porcelain_v2(output: &str) -> (String, u32, u32, u32, u32, u32, u32) {
    let mut branch = String::new();
    let mut oid = "";
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    let mut staged: u32 = 0;
    let mut modified: u32 = 0;
    let mut untracked: u32 = 0;
    let mut conflicted: u32 = 0;

    for line in output.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        match bytes[0] {
            b'#' => {
                if let Some(rest) = line.strip_prefix("# branch.head ") {
                    branch = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
                    for part in rest.split_ascii_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            behind = n.parse().unwrap_or(0);
                        }
                    }
                } else if let Some(rest) = line.strip_prefix("# branch.oid ") {
                    oid = rest;
                }
            }
            b'u' => conflicted += 1,
            b'1' | b'2' if bytes.len() >= 4 && bytes[1] == b' ' => {
                if bytes[2] != b'.' {
                    staged += 1;
                }
                if bytes[3] != b'.' {
                    modified += 1;
                }
            }
            b'?' => untracked += 1,
            _ => {}
        }
    }

    if branch == "(detached)" {
        branch = if oid.len() >= 7 {
            oid[..7].to_string()
        } else if !oid.is_empty() {
            oid.to_string()
        } else {
            "HEAD".to_string()
        };
    }

    (branch, ahead, behind, staged, modified, untracked, conflicted)
}

fn stash_count(common_dir: &Path) -> u32 {
    match std::fs::read(common_dir.join("logs/refs/stash")) {
        Ok(bytes) => bytes.iter().filter(|&&b| b == b'\n').count() as u32,
        Err(_) => 0,
    }
}

fn detect_operation_state(git_dir: &Path) -> OperationState {
    if git_dir.join("MERGE_HEAD").exists() {
        OperationState::Merge
    } else if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        OperationState::Rebase
    } else if git_dir.join("CHERRY_PICK_HEAD").exists() {
        OperationState::CherryPick
    } else if git_dir.join("BISECT_LOG").exists() {
        OperationState::Bisect
    } else if git_dir.join("REVERT_HEAD").exists() {
        OperationState::Revert
    } else {
        OperationState::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clean_repo() {
        let output = "\
# branch.oid abc1234567890
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -0
";
        let (branch, ahead, behind, staged, modified, untracked, conflicted) =
            parse_porcelain_v2(output);
        assert_eq!(branch, "main");
        assert_eq!(ahead, 0);
        assert_eq!(behind, 0);
        assert_eq!(staged, 0);
        assert_eq!(modified, 0);
        assert_eq!(untracked, 0);
        assert_eq!(conflicted, 0);
    }

    #[test]
    fn parse_mixed_changes() {
        let output = "\
# branch.oid abc1234567890
# branch.head feature/test
# branch.upstream origin/feature/test
# branch.ab +3 -1
1 M. N... 100644 100644 100644 abc123 def456 src/main.rs
1 .M N... 100644 100644 100644 abc123 def456 src/lib.rs
1 MM N... 100644 100644 100644 abc123 def456 src/both.rs
? new-file.txt
? another-new.txt
u UU N... 100755 100755 100755 100755 abc123 def456 ghi789 conflict.rs
";
        let (branch, ahead, behind, staged, modified, untracked, conflicted) =
            parse_porcelain_v2(output);
        assert_eq!(branch, "feature/test");
        assert_eq!(ahead, 3);
        assert_eq!(behind, 1);
        assert_eq!(staged, 2); // M. and MM
        assert_eq!(modified, 2); // .M and MM
        assert_eq!(untracked, 2);
        assert_eq!(conflicted, 1);
    }

    #[test]
    fn parse_detached_head() {
        let output = "\
# branch.oid abc1234567890def
# branch.head (detached)
";
        let (branch, _, _, _, _, _, _) = parse_porcelain_v2(output);
        assert_eq!(branch, "abc1234");
    }

    #[test]
    fn parse_renamed_file() {
        let output = "\
# branch.oid abc1234567890
# branch.head main
2 R. N... 100644 100644 100644 abc123 def456 R100 new.rs\told.rs
";
        let (_, _, _, staged, modified, _, _) = parse_porcelain_v2(output);
        assert_eq!(staged, 1);
        assert_eq!(modified, 0);
    }
}
