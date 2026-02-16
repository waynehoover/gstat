use std::fmt::Write;

use crate::types::GitStatus;

pub fn format_json(status: &GitStatus) -> String {
    serde_json::to_string(status).unwrap()
}

pub fn format_custom(status: &GitStatus, template: &str) -> String {
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len + 32);
    let mut ibuf = itoa::Buffer::new();
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'{' => {
                if let Some(end) = template[i + 1..].find('}') {
                    let close = i + 1 + end;
                    let key = &template[i + 1..close];
                    match key {
                        "branch" => result.push_str(&status.branch),
                        "staged" => result.push_str(ibuf.format(status.staged)),
                        "modified" => result.push_str(ibuf.format(status.modified)),
                        "untracked" => result.push_str(ibuf.format(status.untracked)),
                        "conflicted" => result.push_str(ibuf.format(status.conflicted)),
                        "ahead" => result.push_str(ibuf.format(status.ahead)),
                        "behind" => result.push_str(ibuf.format(status.behind)),
                        "stash" => result.push_str(ibuf.format(status.stash)),
                        "state" => {
                            let _ = write!(result, "{}", status.state);
                        }
                        _ => result.push_str(&template[i..close + 1]),
                    }
                    i = close + 1;
                } else {
                    result.push('{');
                    i += 1;
                }
            }
            b'\\' if i + 1 < len => {
                match bytes[i + 1] {
                    b't' => { result.push('\t'); i += 2; }
                    b'n' => { result.push('\n'); i += 2; }
                    _ => { result.push('\\'); i += 1; }
                }
            }
            _ => {
                let start = i;
                i += 1;
                while i < len && bytes[i] != b'{' && bytes[i] != b'\\' {
                    i += 1;
                }
                result.push_str(&template[start..i]);
            }
        }
    }
    result
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
