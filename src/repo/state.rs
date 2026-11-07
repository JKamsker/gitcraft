//! The tab's state types: operations and their undo, open diffs, comparisons, the commit search.

use std::collections::HashSet;
use std::time::Instant;

use super::file_text::FileText;
use super::{diff, short};
use crate::git::{self, StatusFile};

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
    pub(super) id: u64,
    pub(super) label: String,
    pub(super) undo: Vec<Vec<String>>,
    pub(super) redo: Vec<Vec<String>>,
    /// HEAD before and after an operation that moved it: undoing resets whatever branch is
    /// checked out, so it only runs while HEAD is still where the operation left it (and redo
    /// while it is back where it was), not after a checkout or commit made elsewhere since.
    pub(super) heads: Option<(String, String)>,
    /// The agent that asked for it over `gitcraft mcp`, when it wasn't the user: the toolbar
    /// marks it with a robot.
    pub(super) agent: Option<String>,
}

/// An operation as `run_ops_ex` got it, kept to retry it with credentials.
pub(super) struct PendingOp {
    pub(super) label: String,
    pub(super) ops: Vec<Vec<String>>,
    pub(super) refresh: Refresh,
    pub(super) kind: OpKind,
    pub(super) quiet: bool,
}

#[derive(Clone, Debug)]
pub(super) enum OpKind {
    Normal(Undo, String),
    Undo(UndoEntry),
    Redo(UndoEntry),
}

impl OpKind {
    /// An operation that isn't undone.
    pub(super) fn plain() -> Self {
        OpKind::Normal(Undo::None, String::new())
    }
}

/// Which diff the central area shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiffKey {
    Commit {
        hash: String,
        path: String,
    },
    Work(StatusFile),
    /// A file from one commit to another, or to the working tree (`to: None`).
    Range {
        from: String,
        to: Option<String>,
        path: String,
    },
    /// A file of what the merge `hash` resolved: git's own merge of its parents against the
    /// one recorded (see [`crate::git::remerge`]). `old_path`: its name in git's merge;
    /// `conflicts`: what git said of the file's conflicts, as read with the whole merge (a
    /// conflict over several paths is said under one of them only).
    Remerge {
        hash: String,
        path: String,
        old_path: Option<String>,
        conflicts: Vec<String>,
    },
}

impl DiffKey {
    /// The file the diff is of.
    pub(super) fn path(&self) -> &str {
        match self {
            DiffKey::Commit { path, .. }
            | DiffKey::Range { path, .. }
            | DiffKey::Remerge { path, .. } => path,
            DiffKey::Work(file) => &file.path,
        }
    }

    /// Whether the diff can show its Markdown file rendered: not one of what a merge resolved,
    /// whose old side (git's own merge) was never written anywhere to render.
    pub(super) fn renders(&self) -> bool {
        super::diff::rendered::is_markdown(self.path()) && !matches!(self, DiffKey::Remerge { .. })
    }
}

pub(crate) struct DiffView {
    pub(super) key: DiffKey,
    pub(super) data: Option<Result<diff::DiffData, String>>,
    /// Changed lines clicked, to stage, unstage or discard only those.
    pub(super) chosen: std::collections::BTreeSet<usize>,
    /// The last line clicked, where a Shift+click range starts.
    pub(super) anchor: Option<usize>,
    /// Text selected by dragging over the lines, to copy.
    pub(super) text_sel: Option<diff::TextSel>,
    /// The whole file, read when the diff is switched to it.
    pub(super) file: Option<Result<FileText, String>>,
    /// A Markdown file rendered, read when the diff is switched to that.
    pub(super) rendered: Option<Result<diff::rendered::RenderedMd, String>>,
    /// Which reading of the rendered Markdown is the latest: an older one arriving last is
    /// dropped.
    pub(super) rendered_load: u64,
    /// What the rendered Markdown keeps between frames.
    pub(super) markdown: egui_commonmark::CommonMarkCache,
    /// How far the lines were scrolled down last frame.
    pub(super) scroll: f32,
    /// A change (by its place among the changes) to scroll to on the next frame.
    pub(super) jump: Option<usize>,
    /// The change jumped to last, and the scroll offset that left.
    pub(super) at_change: Option<(usize, f32)>,
    /// The latest read of this diff: only its answer is shown, so a slower answer to an older
    /// read (before the whitespace or whole-file setting changed…) can't replace it.
    pub(super) request: u64,
    /// Opened from the history of some lines: only the part of the commit's diff that changed
    /// these lines of the file (as they are in the commit).
    pub(super) lines: Option<Vec<crate::git::LineRange>>,
    /// A folded file's lines (a lockfile…) were opened.
    pub(super) unfolded: bool,
    /// Where the view was when the diff was asked for again another way (another algorithm):
    /// put back at the top once the new reading arrives.
    pub(super) keep_place: Option<diff::Place>,
    /// That place, once the new reading is in: put back at the top in the layout shown then.
    pub(super) restore: Option<diff::Place>,
    /// The algorithm the diff was last asked for with (`None`: git's default).
    pub(super) algorithm: Option<crate::git::DiffAlgorithm>,
    /// The diff is being read again with another algorithm: the lines shown are still the
    /// old pairing.
    pub(super) repairing: bool,
    /// A scroll offset to go to on the next frame.
    pub(super) scroll_to: Option<f32>,
}

/// Numbers the diff reads of every tab, so no two diffs ever wait for the same number.
static DIFF_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl DiffView {
    pub(super) fn new(key: DiffKey) -> Self {
        Self {
            key,
            data: None,
            chosen: Default::default(),
            anchor: None,
            text_sel: None,
            file: None,
            rendered: None,
            rendered_load: 0,
            markdown: Default::default(),
            scroll: 0.0,
            jump: None,
            at_change: None,
            request: 0,
            lines: None,
            unfolded: false,
            keep_place: None,
            restore: None,
            algorithm: None,
            repairing: false,
            scroll_to: None,
        }
    }

    /// Numbers a new read of this diff; from now on only its answer is shown.
    pub(super) fn next_request(&mut self) -> u64 {
        self.request = DIFF_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        self.request
    }

    /// Shows the answer to the read numbered `request`, unless a newer read was made since.
    pub(super) fn arrived(&mut self, request: u64, result: Result<diff::DiffData, String>) {
        if request != self.request {
            return;
        }
        self.repairing = false;
        if result.is_ok() {
            self.restore = self.keep_place.take();
        }
        self.data = Some(result);
        // The lines may have moved.
        self.chosen.clear();
        self.anchor = None;
        self.text_sel = None;
        self.at_change = None;
    }

    /// Shows the whole file read along with the read numbered `request`, unless a newer read
    /// was made since.
    pub(super) fn file_arrived(&mut self, request: u64, result: Result<FileText, String>) {
        if request == self.request {
            self.file = Some(result);
        }
    }

    /// Shows the Markdown rendered read along with the read numbered `request`, unless a newer
    /// read was made since.
    pub(super) fn rendered_arrived(
        &mut self,
        (request, load): (u64, u64),
        result: Result<diff::rendered::RenderedMd, String>,
        ctx: &egui::Context,
    ) {
        if request == self.request && load == self.rendered_load {
            if let Ok(md) = &result {
                diff::rendered::include_images(ctx, md);
            }
            self.rendered = Some(result);
        }
    }
}

/// A commit's message being edited in the right pane (double-click on it).
pub(crate) struct Reword {
    pub hash: String,
    pub subject: String,
    pub body: String,
    /// Put the cursor in the summary on the next frame.
    pub focus: bool,
    /// Opened from the commit's menu: the message goes in once the commit's detail is read.
    pub fill: bool,
}

/// Two points of history side by side in the right pane: a branch against another, a commit
/// against the working tree…
pub(crate) struct Compare {
    pub(super) base: String,
    pub(super) base_label: String,
    /// `None`: the working tree.
    pub(super) target: Option<String>,
    pub(super) target_label: String,
    /// Only what the target changed since the two diverged (from their merge base), like a pull
    /// request shows it.
    pub(super) since_fork: bool,
    pub(super) data: Option<Result<CompareData, String>>,
    /// Numbers the requests, so an older answer doesn't replace a newer one.
    pub(super) request: u64,
}

pub(crate) struct CompareData {
    /// Where the diff starts: the base's commit, or the merge base.
    pub(super) from: String,
    /// The target's commit (`None`: the working tree).
    pub(super) to: Option<String>,
    pub(super) files: Vec<git::ChangedFile>,
    /// Commits only the base has, and only the target (HEAD for the working tree) has.
    pub(super) apart: (u32, u32),
}

impl Compare {
    /// How the comparison calls one of its two ends.
    pub(super) fn name_of(&self, rev: &str) -> Option<String> {
        let data = self.data.as_ref()?.as_ref().ok()?;
        if data.to.as_deref() == Some(rev) {
            Some(self.target_label.clone())
        } else if data.from == rev && self.since_fork {
            Some(format!("merge base {}", short(rev)))
        } else if data.from == rev {
            Some(self.base_label.clone())
        } else {
            None
        }
    }
}

#[derive(Default)]
pub(super) struct Search {
    pub(super) open: bool,
    pub(super) focus: bool,
    /// Search what commits added or removed in files (`git log -S`), not their messages.
    pub(super) code: bool,
    pub(super) query: String,
    /// The query the results are for.
    pub(super) searched: String,
    pub(super) results: Vec<String>,
    pub(super) matches: HashSet<String>,
    pub(super) current: usize,
    pub(super) running: bool,
    pub(super) edited: Option<Instant>,
    /// Why the query can't be searched (its operators written wrong), or what git said.
    pub(super) problem: Option<String>,
    /// What the field suggests while an operator is typed.
    pub(super) suggest: super::graph_view::suggest::SuggestState,
}
