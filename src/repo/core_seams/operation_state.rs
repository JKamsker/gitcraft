/// What to re-read after an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refresh {
    /// Only the working tree (staging changes nothing else).
    Status,
    All,
}

/// How an operation can be undone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Undo {
    None,
    /// The operation moved HEAD (commit, merge, reset…): undo moves it back with
    /// `git reset <mode>`, redo forward again. `--soft` keeps the changes staged.
    HeadMove {
        mode: &'static str,
    },
    /// Explicit commands each way (branch create/delete/rename, checkout, tags, stash).
    Custom {
        undo: Vec<Vec<String>>,
        redo: Vec<Vec<String>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct UndoEntry {
    /// The timeline entry this undoes (see `timeline::Entry::id`); 0 until it has one.
    #[serde(default)]
    pub(in crate::repo) id: u64,
    pub(in crate::repo) label: String,
    pub(in crate::repo) undo: Vec<Vec<String>>,
    pub(in crate::repo) redo: Vec<Vec<String>>,
    /// HEAD before and after an operation that moved it: undoing resets whatever branch is
    /// checked out, so it only runs while HEAD is still where the operation left it (and redo
    /// while it is back where it was), not after a checkout or commit made elsewhere since.
    pub(in crate::repo) heads: Option<(String, String)>,
    /// The agent that asked for it over `gitcraft mcp`, when it wasn't the user: the toolbar
    /// marks it with a robot.
    #[serde(default)]
    pub(in crate::repo) agent: Option<String>,
}

/// An operation as `run_ops_ex` got it, kept to retry it with credentials.
pub(in crate::repo) struct PendingOp {
    pub(in crate::repo) label: String,
    pub(in crate::repo) ops: Vec<Vec<String>>,
    pub(in crate::repo) refresh: Refresh,
    pub(in crate::repo) kind: OpKind,
    pub(in crate::repo) quiet: bool,
}

#[derive(Clone, Debug)]
pub(in crate::repo) enum OpKind {
    Normal(Undo, String),
    Undo(UndoEntry),
    Redo(UndoEntry),
}

impl OpKind {
    /// An operation that isn't undone.
    pub(in crate::repo) fn plain() -> Self {
        OpKind::Normal(Undo::None, String::new())
    }
}
