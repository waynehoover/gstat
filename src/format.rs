use crate::types::GitStatus;

pub fn format_json(status: &GitStatus) -> String {
    serde_json::to_string(status).unwrap()
}

pub fn format_custom(status: &GitStatus, template: &str) -> String {
    template
        .replace("{branch}", &status.branch)
        .replace("{staged}", &status.staged.to_string())
        .replace("{modified}", &status.modified.to_string())
        .replace("{untracked}", &status.untracked.to_string())
        .replace("{conflicted}", &status.conflicted.to_string())
        .replace("{ahead}", &status.ahead.to_string())
        .replace("{behind}", &status.behind.to_string())
        .replace("{stash}", &status.stash.to_string())
        .replace("{state}", &status.state.to_string())
        .replace("\\t", "\t")
        .replace("\\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OperationState;

    fn sample_status() -> GitStatus {
        GitStatus {
            branch: "main".to_string(),
            staged: 2,
            modified: 3,
            untracked: 1,
            conflicted: 0,
            ahead: 1,
            behind: 0,
            stash: 2,
            state: OperationState::Clean,
        }
    }

    #[test]
    fn json_output() {
        let s = sample_status();
        let json = format_json(&s);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["branch"], "main");
        assert_eq!(parsed["staged"], 2);
        assert_eq!(parsed["modified"], 3);
        assert_eq!(parsed["untracked"], 1);
        assert_eq!(parsed["conflicted"], 0);
        assert_eq!(parsed["ahead"], 1);
        assert_eq!(parsed["behind"], 0);
        assert_eq!(parsed["stash"], 2);
        assert_eq!(parsed["state"], "clean");
    }

    #[test]
    fn custom_format() {
        let s = sample_status();
        let result = format_custom(&s, " {branch} +{staged} ~{modified} ?{untracked} ⇡{ahead}⇣{behind}");
        assert_eq!(result, " main +2 ~3 ?1 ⇡1⇣0");
    }

    #[test]
    fn custom_format_with_state() {
        let mut s = sample_status();
        s.state = OperationState::Rebase;
        let result = format_custom(&s, "{branch}|{state}");
        assert_eq!(result, "main|rebase");
    }

    #[test]
    fn custom_format_clean_state_empty() {
        let s = sample_status();
        let result = format_custom(&s, "{branch}{state}");
        assert_eq!(result, "main");
    }

    #[test]
    fn custom_format_tab_separated() {
        let s = sample_status();
        let result = format_custom(
            &s,
            "{branch}\\t{staged}\\t{modified}\\t{untracked}\\t{conflicted}\\t{ahead}\\t{behind}\\t{stash}\\t{state}",
        );
        assert_eq!(result, "main\t2\t3\t1\t0\t1\t0\t2\t");
    }
}
