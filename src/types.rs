use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitStatus {
    pub branch: String,
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub ahead: u32,
    pub behind: u32,
    pub stash: u32,
    pub state: OperationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Clean,
    Merge,
    Rebase,
    CherryPick,
    Bisect,
    Revert,
}

impl fmt::Display for OperationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OperationState::Clean => write!(f, ""),
            OperationState::Merge => write!(f, "merge"),
            OperationState::Rebase => write!(f, "rebase"),
            OperationState::CherryPick => write!(f, "cherry-pick"),
            OperationState::Bisect => write!(f, "bisect"),
            OperationState::Revert => write!(f, "revert"),
        }
    }
}
