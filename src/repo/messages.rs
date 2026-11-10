//! Results of background work coming back to the tab, and the per-frame polling that takes
//! them in (with file-system events, the background fetch and the debounced search).

#[path = "core_seams/job_submission.rs"]
mod job_submission;
#[allow(unused_imports)]
pub(crate) use job_submission::*;

#[path = "core_seams/message_dispatch.rs"]
mod message_dispatch;
#[allow(unused_imports)]
pub(crate) use message_dispatch::*;

#[path = "core_seams/operation_labels.rs"]
mod operation_labels;
#[allow(unused_imports)]
pub(crate) use operation_labels::*;

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::file_text::FileText;
use super::load::Loaded;
use super::watch::Reread;
use super::{CompareData, OpKind, Refresh, RepoTab, diff, file_view, ops, pr_review, quick_edit};
use crate::forge::{Hosting, PullRequest};
use crate::git::{self, Commit, CommitDetail, WorkStatus};
use crate::settings::Settings;

/// A tab's result channel and the lifetime inherited by all of its jobs.
#[derive(Clone)]
pub(super) struct JobSender {
    pub(super) sender: Sender<Msg>,
    pub(super) forge_generation: Option<u64>,
    pub(super) forge_operation: bool,
    pub(super) cancel: crate::tasks::Cancellation,
    pub(super) repo: std::path::PathBuf,
}

impl JobSender {
    pub(super) fn send(&self, msg: Msg) -> Result<(), Box<std::sync::mpsc::SendError<Msg>>> {
        let msg = match self.forge_generation {
            Some(generation) => Msg::ForgeResult(generation, self.forge_operation, Box::new(msg)),
            None => msg,
        };
        self.sender.send(msg).map_err(Box::new)
    }
}

impl std::ops::Deref for JobSender {
    type Target = Sender<Msg>;
    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

/// Scheduling inputs are lexical only: submission must never inspect a filesystem.
/// The pool classifies every local path into one local mutation bucket.
pub(crate) fn mutation_key(path: &std::path::Path) -> std::path::PathBuf {
    path.to_owned()
}

static FETCH_KEYS: std::sync::Mutex<Vec<(std::path::PathBuf, std::path::PathBuf)>> =
    std::sync::Mutex::new(Vec::new());

/// Cached ownership identity, separate from scheduling. Before discovery, each
/// path owns itself; background reads refine linked-worktree fetch deduplication.
pub(crate) fn fetch_key(path: &std::path::Path) -> std::path::PathBuf {
    FETCH_KEYS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(p, _)| p == path)
        .map_or_else(|| path.to_owned(), |(_, key)| key.clone())
}

/// Called only from background repository/tray reads, never during submission.
pub(crate) fn discover_fetch_key(path: &std::path::Path) {
    let key = if crate::remote::is_remote(path) {
        path.to_owned()
    } else {
        super::watch::linked_git_dirs(path)
            .map(|(_, common)| common)
            .or_else(|| std::fs::canonicalize(path.join(".git")).ok())
            .unwrap_or_else(|| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()))
    };
    let mut keys = FETCH_KEYS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, cached)) = keys.iter_mut().find(|(p, _)| p == path) {
        *cached = key;
    } else {
        if keys.len() >= 1024 {
            keys.remove(0);
        }
        keys.push((path.to_owned(), key));
    }
}

/// What a background job leaves to do on the UI thread, with the tab (see `RepoTab::then`).
pub(super) type Continuation = Box<dyn FnOnce(&mut RepoTab, &egui::Context) + Send>;

/// Results coming back from background git calls.
pub(super) enum Msg {
    /// A hosting job belongs to one selection lifetime, including switching away and back.
    ForgeResult(u64, bool, Box<Msg>),
    Loaded(Result<Box<Loaded>, String>),
    /// The submodules, from the read numbered so (see `RepoTab::reads`).
    Submodules(u64, Vec<git::Submodule>),
    /// Who commits are made as, read again (the profile's author changed).
    Identity(git::Identity),
    /// The refs as they are now, to compare with those read (see `RepoTab::recheck`).
    RefsCheck(Result<git::Refs, String>),
    /// What to do on the UI thread with what a background job found (see `RepoTab::then`).
    Then(Continuation),
    /// A conflict editor read in the background (for a repository on another machine), for
    /// the opening numbered so (see `RepoTab::conflict_generation`).
    ConflictOpened(u64, String, Option<Box<super::conflict::ConflictEditor>>),
    /// The file of the conflict editor opened so changed on disk (`None`: it didn't, or it
    /// couldn't be read).
    ConflictChanged(u64, Option<String>),
    /// A conflict editor's file written in the background, to be marked resolved.
    ConflictSaved(String, Result<(), String>),
    /// The changes of each worktree (by its path) counted after a read (see
    /// `RepoTab::count_worktrees`).
    WorktreeCounts(Vec<(std::path::PathBuf, usize)>),
    /// The worktree's watcher, set up in the background (`None` when it couldn't be).
    /// The watcher set up by the `watch` numbered so (see `RepoTab::watches`).
    Watcher(u64, Option<Box<notify::RecommendedWatcher>>),
    /// A further page of history, read while `history_gen` was so: a reload since may have
    /// moved the commits it skipped, so then it no longer fits.
    More(u64, Vec<String>, Result<Vec<Commit>, String>),
    /// The working tree, from the read numbered so (see `RepoTab::reads`).
    Status(u64, Result<WorkStatus, String>),
    Detail(String, Result<CommitDetail, String>),
    /// A neighbour of the selected commit, read ahead for the detail cache.
    DetailPrefetched(Result<CommitDetail, String>),
    /// The files of the commits asked for (those on screen), read ahead in one batch.
    FilesPrefetched(
        Vec<String>,
        Result<Vec<(String, Vec<git::ChangedFile>)>, String>,
    ),
    /// The answer to the diff request numbered so (see [`DiffView::next_request`]).
    ///
    /// [`DiffView::next_request`]: super::DiffView::next_request
    Diff(u64, Result<Box<diff::DiffData>, String>),
    /// The whole file of the diff, read along with the diff request numbered so.
    DiffFile(u64, Result<FileText, String>),
    /// The Markdown file of the diff rendered, read along with the diff request numbered so.
    /// The diff's request, and which reading of it.
    DiffRendered((u64, u64), Box<Result<diff::rendered::RenderedMd, String>>),
    /// The diff read as a rich view, along with the diff request numbered so.
    DiffRich(
        (u64, u64),
        diff::rich::RichKind,
        Box<Result<diff::rich::Rich, String>>,
    ),
    DiffStructural(
        (u64, u64),
        Box<Result<diff::structural::Structural, String>>,
    ),
    FileHistory(
        u64,
        String,
        Option<String>,
        Result<Vec<git::FileCommit>, String>,
    ),
    Blame(
        u64,
        String,
        Option<String>,
        Result<file_view::BlameData, String>,
    ),
    /// A file as it is at a revision, for the file view.
    FileText(u64, String, Option<String>, Result<FileText, String>),
    /// Every file in the tree of a commit.
    TreeFiles(String, Result<Vec<String>, String>),
    RebaseCommits {
        onto: String,
        label: String,
        result: Result<Vec<git::RebaseCommit>, String>,
    },
    /// Results for a query, searched in the code (`true`) or in the messages.
    Search(Vec<String>, String, bool, Result<Vec<String>, String>),
    /// A comparison's files, for the request numbered so.
    Compare(u64, Result<CompareData, String>),
    /// The open pull requests, as asked for at that moment (see `RepoTab::prs_loaded`).
    Prs(Instant, Result<Vec<PullRequest>, String>),
    /// Build statuses by commit, as each project said them; `true` when new commits were
    /// asked about.
    CiStatuses(bool, crate::ci::route::Answers),
    /// The runs of the commit the pipeline view shows, for the read numbered so.
    CiRuns(u64, Result<super::ci_view::Builds, String>),
    /// The jobs of a run (by id), for the read numbered so.
    CiJobs(u64, u64, Result<Vec<crate::ci::Job>, String>),
    CiLog(u64, Result<String, String>),
    /// A run was re-run or cancelled, or wasn't.
    CiActed(String, Result<(), String>),
    /// A pull request in full, for the request numbered so.
    PrDetails(u64, Box<Result<crate::pull_request::PrDetails, String>>),
    PrFiles(u64, Result<crate::pull_request::PrFiles, String>),
    /// Something said on a pull request went out, or didn't.
    PrPosted {
        /// Where it was sent: an answer from a repository the tab has moved away from is no answer.
        hosting: Box<Hosting>,
        number: u64,
        what: pr_review::Posted,
        result: Result<(), String>,
    },
    /// The commits to replay when rewording `hash`.
    RewordCommits {
        hash: String,
        message: String,
        result: Result<Vec<git::RebaseCommit>, String>,
    },
    /// A commit's patch was written to the file, or wasn't.
    PatchSaved(std::path::PathBuf, Result<(), String>),
    BundleDone(String, Result<(), String>, Refresh),
    /// The commits to replay for a drop or move from a commit's menu.
    QuickEditCommits {
        edit: quick_edit::Edit,
        hash: String,
        branch_guard: Vec<String>,
        result: Result<Vec<git::RebaseCommit>, String>,
    },
    OpDone {
        label: String,
        result: Result<String, String>,
        refresh: Refresh,
        kind: OpKind,
        quiet: bool,
        /// The step it stopped at: those before it ran.
        stopped: usize,
    },
    /// Mergiraf's take on a conflicted file.
    Mergiraf(String, Result<String, String>),
    /// Work started with [`spawn`] panicked (a bug, perhaps in a dependency): what it said.
    JobPanicked(String),
}

/// Runs `job` on a background thread and wakes the UI when its message is sent. Should it
/// panic, [`Msg::JobPanicked`] comes instead: for work the tab waits for (an operation, a
/// reload…), [`spawn_or`] says what to send then, so that the tab doesn't wait forever.
mod review;
mod workflow;

#[cfg(test)]
#[path = "direct_tests/messages_fluent_presentation_tests.rs"]
mod fluent_presentation_tests;

#[path = "core_seams/label_templates.rs"]
mod label_templates;
use label_templates::*;
