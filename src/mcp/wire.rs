//! What an agent asks the window for, and how it ended: carried in the single-instance
//! channel's `agent` requests (see `crate::instance::message`).
//!
//! The request says what the agent asks for ([`WriteOp`]), never how risky it is: the window
//! decides itself which operations wait for the user's approval.

use serde::{Deserialize, Serialize};

/// How a reset moves the branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    /// Keeps every change, staged.
    Soft,
    /// Keeps every change, unstaged.
    Mixed,
    /// Throws away uncommitted changes: waits for the user's approval.
    Hard,
}

impl ResetMode {
    pub fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

/// An operation an agent asks the window to run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WriteOp {
    Stage {
        paths: Vec<String>,
    },
    Unstage {
        paths: Vec<String>,
    },
    Commit {
        message: String,
        amend: bool,
    },
    CreateBranch {
        name: String,
        /// A revision; HEAD when `None`.
        start: Option<String>,
        checkout: bool,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    Checkout {
        branch: String,
    },
    Stash {
        message: String,
    },
    CherryPick {
        commit: String,
    },
    Revert {
        commit: String,
    },
    Reset {
        commit: String,
        mode: ResetMode,
    },
    /// Swaps a commit of the checked-out branch with the one after it (`up`) or before it.
    MoveCommit {
        commit: String,
        up: bool,
    },
    DropCommit {
        commit: String,
    },
    DeleteBranch {
        name: String,
        /// Even if it isn't merged (`-D`).
        force: bool,
    },
    /// Pushes the checked-out branch.
    Push {
        /// `--force-with-lease`.
        force: bool,
    },
}

impl WriteOp {
    /// Whether the user has to approve it in the window first: what can't be undone there, or
    /// throws work away for good.
    pub fn needs_approval(&self) -> bool {
        matches!(
            self,
            WriteOp::Reset {
                mode: ResetMode::Hard,
                ..
            } | WriteOp::DropCommit { .. }
                | WriteOp::DeleteBranch { .. }
                | WriteOp::Push { .. }
        )
    }
}

/// How a request ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Failed,
    /// The user said no.
    Denied,
    /// Nobody answered the approval card in time, or the window didn't get to it.
    TimedOut,
    /// This window has no tab for the repository.
    NotOpen,
    /// The user turned agents' changes off in the settings.
    Disabled,
    /// Not a request this window understands (another version, a wrong token).
    BadRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub outcome: Outcome,
    pub message: String,
}

impl Reply {
    pub fn new(outcome: Outcome, message: impl Into<String>) -> Self {
        Self {
            outcome,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_operation_reads_back_as_written() {
        let op = WriteOp::Reset {
            commit: "abc".into(),
            mode: ResetMode::Hard,
        };
        let text = serde_json::to_string(&op).unwrap_or_default();
        assert_eq!(text, r#"{"op":"reset","commit":"abc","mode":"hard"}"#);
        assert_eq!(serde_json::from_str::<WriteOp>(&text).ok(), Some(op));
    }

    #[test]
    fn only_what_cant_be_undone_waits_for_approval() {
        let asks = |op: WriteOp| op.needs_approval();
        assert!(asks(WriteOp::Push { force: false }));
        assert!(asks(WriteOp::DeleteBranch {
            name: "x".into(),
            force: false
        }));
        assert!(asks(WriteOp::DropCommit { commit: "c".into() }));
        assert!(asks(WriteOp::Reset {
            commit: "c".into(),
            mode: ResetMode::Hard
        }));
        assert!(!asks(WriteOp::Reset {
            commit: "c".into(),
            mode: ResetMode::Soft
        }));
        assert!(!asks(WriteOp::Commit {
            message: "m".into(),
            amend: true
        }));
        assert!(!asks(WriteOp::Stage { paths: vec![] }));
    }
}
