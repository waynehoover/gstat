use std::path::Path;
use std::process::Command;

use crate::types::{GitStatus, OperationState};

pub fn compute_status(repo_root: &Path) -> GitStatus {
    let porcelain = run_git(repo_root, &["status", "--porcelain=v2", "--branch"]);
    let (branch, ahead, behind, staged, modified, untracked, conflicted) =
        parse_porcelain_v2(&porcelain);

    let stash = stash_count(repo_root);
    let state = detect_operation_state(repo_root);

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
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn parse_porcelain_v2(output: &str) -> (String, u32, u32, u32, u32, u32, u32) {
    let mut branch = String::new();
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    let mut staged: u32 = 0;
    let mut modified: u32 = 0;
    let mut untracked: u32 = 0;
    let mut conflicted: u32 = 0;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            // Format: "+N -M"
            for part in rest.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("# ") {
            // Other header lines, skip
        } else if line.starts_with("u ") {
            // Unmerged entry
            conflicted += 1;
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            // Changed entry: "1 XY ..." or renamed "2 XY ..."
            let xy = line.split_whitespace().nth(1).unwrap_or("..");
            let bytes = xy.as_bytes();
            let x = bytes.first().copied().unwrap_or(b'.');
            let y = bytes.get(1).copied().unwrap_or(b'.');
            if x != b'.' {
                staged += 1;
            }
            if y != b'.' {
                modified += 1;
            }
        } else if line.starts_with("? ") {
            untracked += 1;
        }
    }

    // Detached HEAD: branch.head is "(detached)"
    if branch == "(detached)" {
        branch = detached_short_hash(output);
    }

    (branch, ahead, behind, staged, modified, untracked, conflicted)
}

fn detached_short_hash(output: &str) -> String {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.oid ") {
            if rest.len() >= 7 {
                return rest[..7].to_string();
            }
            return rest.to_string();
        }
    }
    "HEAD".to_string()
}

fn stash_count(repo_root: &Path) -> u32 {
    let output = Command::new("git")
        .args(["rev-list", "--count", "refs/stash"])
        .current_dir(repo_root)
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

fn detect_operation_state(repo_root: &Path) -> OperationState {
    let git_dir = repo_root.join(".git");

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
