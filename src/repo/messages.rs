//! Results of background work coming back to the tab, and the per-frame polling that takes
//! them in (with file-system events, the background fetch and the debounced search).

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
    pub(super) cancel: crate::tasks::Cancellation,
    pub(super) repo: std::path::PathBuf,
}

impl std::ops::Deref for JobSender {
    type Target = Sender<Msg>;
    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

/// Linked local worktrees share refs/stashes, hence one mutation key. Resolve the
/// existing .git/commondir metadata without starting a process on the UI thread.
pub(crate) fn mutation_key(path: &std::path::Path) -> std::path::PathBuf {
    if crate::remote::is_remote(path) {
        return path.to_owned();
    }
    super::watch::linked_git_dirs(path)
        .map(|(_, common)| common)
        .or_else(|| std::fs::canonicalize(path.join(".git")).ok())
        .unwrap_or_else(|| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()))
}

/// What a background job leaves to do on the UI thread, with the tab (see `RepoTab::then`).
pub(super) type Continuation = Box<dyn FnOnce(&mut RepoTab, &egui::Context) + Send>;

/// Results coming back from background git calls.
pub(super) enum Msg {
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
    More(u64, Result<Vec<Commit>, String>),
    /// The working tree, from the read numbered so (see `RepoTab::reads`).
    Status(u64, Result<WorkStatus, String>),
    Detail(String, Result<CommitDetail, String>),
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
    FileHistory(String, Option<String>, Result<Vec<git::FileCommit>, String>),
    Blame(String, Option<String>, Result<file_view::BlameData, String>),
    /// A file as it is at a revision, for the file view.
    FileText(String, Option<String>, Result<FileText, String>),
    /// Every file in the tree of a commit.
    TreeFiles(String, Result<Vec<String>, String>),
    RebaseCommits {
        onto: String,
        label: String,
        result: Result<Vec<git::RebaseCommit>, String>,
    },
    /// Results for a query, searched in the code (`true`) or in the messages.
    Search(String, bool, Result<Vec<String>, String>),
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
pub(super) fn spawn(
    tx: &JobSender,
    ctx: &egui::Context,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    spawn_or(tx, ctx, Msg::JobPanicked, job);
}

impl RepoTab {
    /// Like `then`, but rejection, cancellation and panic have a feature-specific result.
    pub(super) fn then_or<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        mutation: bool,
        failed: impl FnOnce(String) -> T + Send + 'static,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        let then = std::sync::Arc::new(std::sync::Mutex::new(Some(then)));
        let failure_then = std::sync::Arc::clone(&then);
        let failure = move |error| {
            let found = failed(error);
            Msg::Then(Box::new(move |tab, ctx| {
                if let Some(then) = failure_then
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    then(tab, found, ctx);
                }
            }))
        };
        let job = move || {
            let found = work();
            Msg::Then(Box::new(move |tab, ctx| {
                if let Some(then) = then
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    then(tab, found, ctx);
                }
            }))
        };
        if mutation {
            spawn_mutation(&self.tx, ctx, failure, job);
        } else {
            spawn_or(&self.tx, ctx, failure, job);
        }
    }

    /// Runs `work` in the background, then `then` with what it gave, on the UI thread.
    pub(super) fn then<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        spawn(&self.tx, ctx, move || {
            let found = work();
            Msg::Then(Box::new(move |tab, ctx| then(tab, found, ctx)))
        });
    }

    /// [`Self::then`] for a repository on another machine, where `work` is a round trip or
    /// more (and may wait for a connection); for one here, right away, as it always was.
    pub(super) fn here_or_later<T: Send + 'static>(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        if crate::remote::is_remote(&self.path) {
            self.then(ctx, work, then);
        } else {
            let found = work();
            then(self, found, ctx);
        }
    }
}

/// [`spawn`], sending what `failed` makes of what `job` said if it panics: the message it
/// would have sent had it failed, so that whatever waits for that message (the busy
/// operation, the loading flag…) is let go of.
pub(super) fn spawn_or(
    tx: &JobSender,
    ctx: &egui::Context,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    spawn_with(
        tx,
        ctx,
        crate::tasks::Priority::Interactive,
        None,
        failed,
        job,
    );
}

/// Serializes a whole mutation operation across tabs of the same repository.
pub(super) fn spawn_mutation(
    tx: &JobSender,
    ctx: &egui::Context,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    spawn_with(
        tx,
        ctx,
        crate::tasks::Priority::Background,
        Some(tx.repo.clone()),
        failed,
        job,
    );
}

fn spawn_with(
    tx: &JobSender,
    ctx: &egui::Context,
    priority: crate::tasks::Priority,
    key: Option<std::path::PathBuf>,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    let tx = tx.clone();
    let ctx = ctx.clone();
    let rejected_tx = tx.clone();
    let rejected_ctx = ctx.clone();
    let failed = std::sync::Arc::new(std::sync::Mutex::new(Some(failed)));
    let rejected = std::sync::Arc::clone(&failed);
    let cancelled = std::sync::Arc::clone(&failed);
    let cancelled_tx = tx.clone();
    let cancelled_ctx = ctx.clone();
    let submitted = crate::tasks::pool().and_then(|pool| {
        pool.submit_cancellable(
            tx.cancel.clone(),
            priority,
            key,
            Some(Box::new(move || {
                if let Some(failed) = cancelled
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    let _ = cancelled_tx.send(failed(crate::i18n::text("repo-core-2896").into()));
                    cancelled_ctx.request_repaint();
                }
            })),
            Box::new(move || {
                // What the job owned goes with it; a lock it held is left poisoned, which those
                // taking it handle.
                let msg = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)) {
                    Ok(msg) => msg,
                    Err(payload) => {
                        let Some(failed) = failed
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .take()
                        else {
                            return;
                        };
                        failed(panic_text(payload.as_ref()))
                    }
                };
                let _ = tx.send(msg);
                ctx.request_repaint();
            }),
        )
        .map_err(str::to_owned)
    });
    if let Err(error) = submitted
        && let Some(failed) = rejected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    {
        let _ = rejected_tx.send(failed(error));
        rejected_ctx.request_repaint();
    }
}

/// What a panic said, as an error to show.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    let said = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| crate::i18n::text("repo-core-2897").to_owned());
    crate::i18n::message("repo-core-2898", &[("said", format!("{}", said).into())])
}

impl RepoTab {
    /// Drains finished background work and file-system events.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.poll_as(ctx, true);
    }

    /// [`Self::poll`] for a worktree view parked behind the one shown: its own work is seen
    /// through, but it doesn't fetch, follow builds or reload for changes on disk. The view
    /// shown does that for the repository they share, and this one reloads when shown again.
    pub(crate) fn poll_parked_view(&mut self, ctx: &egui::Context) {
        self.poll_as(ctx, false);
    }

    fn poll_as(&mut self, ctx: &egui::Context, shown: bool) {
        self.notice_connections(ctx);
        self.local_ci.poll(ctx);
        self.shown = shown;
        if !shown || self.fs_pending.is_some() {
            self.discard_decrypted();
        }
        if let Some(poller) = &mut self.poller {
            poller.tick(ctx);
        }
        // Changes on disk, debounced. Taken in before the finished work, so that a reload
        // that work starts covers what it wrote itself (see `reload`).
        if let Some(rx) = &self.fs_rx {
            while let Ok(change) = rx.try_recv() {
                self.working_drafts.observe();
                let was = self.fs_pending.map(|(c, _)| c);
                self.fs_pending = Some((was.max(Some(change)).unwrap_or(change), Instant::now()));
            }
        }

        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg, ctx);
        }
        if !self.sleeping() {
            self.sync_branch_bands(ctx);
            self.poll_branch_bands(ctx);
            self.poll_graph_topology(ctx);
            self.poll_postcards(ctx);
            self.poll_history_activity(ctx);
        }
        self.agent_step(ctx);
        self.poll_outbox(ctx);
        self.poll_deferred(ctx);
        // A tab not read yet, or suspended, neither fetches nor follows builds.
        self.poll_saved_views(ctx);
        self.poll_content_recovery(ctx);
        self.poll_line_dossier(ctx);
        self.poll_symbol_biography(ctx);
        self.poll_cochanges(ctx);
        self.poll_working_drafts(ctx);
        if !shown || self.sleeping() {
            return;
        }
        if let Some(wait) = self.folding.tick(Instant::now()) {
            ctx.request_repaint_after(wait);
        }
        if let Some((change, at)) = self.fs_pending
            && at.elapsed() > Duration::from_millis(400)
            && !self.loading
            && self.busy_op.is_none()
        {
            self.fs_pending = None;
            for reread in change.rereads() {
                match reread {
                    Reread::All => self.reload(ctx),
                    Reread::Status => self.reload_status(ctx),
                    Reread::Submodules => self.reload_submodules(ctx),
                }
            }
        }

        // Background fetch.
        let minutes = Settings::get(ctx).auto_fetch_minutes;
        if minutes > 0
            && !crate::tray::watch::owns_fetch(&self.tx.repo)
            && !self.remotes.is_empty()
            && self.busy_op.is_none()
            && super::offline::background_fetch_may_run(&self.path)
            && self.last_fetch.elapsed() > Duration::from_secs(u64::from(minutes) * 60)
        {
            self.last_fetch = Instant::now();
            self.run_ops_ex(
                "Background fetch",
                ops(&[&["fetch", "--all", "--prune", "--quiet"]]),
                Refresh::All,
                OpKind::plain(),
                true,
                ctx,
            );
        }
        if minutes > 0 {
            ctx.request_repaint_after(Duration::from_secs(30));
        }

        self.follow_network(ctx);
        self.poll_ci(ctx);
        self.poll_parallel(ctx);
        self.recount_siblings(ctx);
        self.poll_ci_view(ctx);

        // Debounced search while typing.
        if let Some(at) = self.search.edited
            && at.elapsed() > Duration::from_millis(350)
        {
            self.run_search(ctx);
        } else if self.search.edited.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(super) fn handle(&mut self, msg: Msg, ctx: &egui::Context) {
        match msg {
            Msg::Loaded(result) => self.loaded(result, ctx),
            Msg::Submodules(read, list) => {
                if Self::newer(read, &mut self.submodules_read) {
                    self.submodules = list;
                }
            }
            Msg::WorktreeCounts(counts) => self.worktrees_counted(counts, ctx),
            Msg::ConflictOpened(generation, path, editor) => {
                self.conflict_opened(generation, path, editor)
            }
            Msg::ConflictChanged(generation, Some(path)) => {
                self.conflict_changed(generation, path, ctx)
            }
            Msg::ConflictChanged(_, None) => {}
            Msg::ConflictSaved(path, saved) => {
                // Editable again (if still open), whatever came of it.
                if let Some(ed) = self.conflict.as_mut().filter(|ed| ed.path == path) {
                    ed.saving = false;
                }
                self.conflict_saved(path, saved, ctx)
            }
            Msg::Identity(identity) => {
                self.identity = identity;
                self.saved_views.dirty = true;
            }
            // One asked for before the tab was suspended (or watched again) sends to a channel
            // no longer read: dropped, not kept.
            Msg::Watcher(watch, watcher) => {
                if watch == self.watches && !self.sleeping() {
                    self.watcher = watcher.map(|w| *w);
                    self.watcher_arrived(ctx);
                }
            }
            Msg::RefsCheck(refs) => {
                // Moved before the watcher could see it.
                if refs.is_ok_and(|refs| refs != self.refs) {
                    self.reload(ctx);
                }
            }
            Msg::More(generation, _) if generation != self.history_gen => {}
            Msg::More(_, result) => self.more_loaded(result, ctx),
            // Older than the working tree shown (a reload started since came in first).
            Msg::Status(read, _) if read <= self.work_read => {}
            Msg::Status(read, result) => match result {
                Ok(work) => {
                    if read >= self.work_pending {
                        self.work_pending = 0;
                    }
                    self.work_read = read;
                    // This worktree's count of changes, as the worktree list shows it.
                    let changes = work.changed_files();
                    if let Some(here) = self.worktrees.iter_mut().find(|wt| wt.current) {
                        here.changes = Some(changes);
                    }
                    let wip_row_changed = work.is_clean() != self.work.is_clean();
                    self.folding
                        .saw_status(&work, &self.refs.head, Instant::now());
                    self.work = work;
                    self.stash_picks.keep_changed(&self.work);
                    if wip_row_changed {
                        self.rebuild(ctx);
                    } else {
                        // The rows are the same: only what shows the working tree needs
                        // refreshing, not the whole graph.
                        self.refresh_work(ctx);
                    }
                    if self.compare.as_ref().is_some_and(|c| c.target.is_none()) {
                        self.reload_compare(ctx);
                    }
                }
                Err(e) => self.status = Some((e, true)),
            },
            Msg::Detail(hash, result) => {
                if self.detail_hash.as_deref() == Some(hash.as_str()) {
                    match result {
                        Ok(d) => {
                            self.detail = Some(d);
                            self.fill_reword();
                        }
                        Err(e) => self.detail_error = Some(e),
                    }
                    self.finish_commit_changes(&hash, ctx);
                }
            }
            Msg::RebaseCommits {
                onto,
                label,
                result,
            } => self.open_rebase(onto, label, result),
            Msg::RewordCommits {
                hash,
                message,
                result,
            } => self.reword_older(hash, message, result, ctx),
            Msg::PatchSaved(file, result) => self.patch_saved(file, result),
            Msg::BundleDone(label, result, refresh) => {
                self.bundle_done(label, result, refresh, ctx)
            }
            Msg::QuickEditCommits { edit, hash, result } => {
                self.run_quick_edit(edit, hash, result, ctx)
            }
            Msg::FileHistory(path, rev, result) => {
                // Only the history of the revision shown: an older read may answer last. Nor
                // the whole file's, once the history of some of its lines is shown.
                if let Some(view) = &mut self.file_view
                    && view.path == path
                    && view.rev == rev
                    && view.lines.is_none()
                {
                    view.history = Some(result);
                }
            }
            Msg::Blame(path, rev, result) => {
                if let Some(view) = &mut self.file_view
                    && view.path == path
                    && view.rev == rev
                {
                    view.blame = Some(result);
                }
            }
            Msg::FileText(path, rev, result) => {
                if let Some(view) = &mut self.file_view
                    && view.path == path
                    && view.rev == rev
                {
                    view.text = Some(result);
                }
            }
            Msg::TreeFiles(hash, result) => self.all_files.arrived(&hash, result),
            Msg::Diff(request, result) => {
                let mut reread = false;
                if let Some(view) = &mut self.diff {
                    // The rich view is read again only when the change itself changed.
                    let source = |d: &Option<Result<diff::DiffData, String>>| match d {
                        Some(Ok(d)) => Some(d.source),
                        _ => None,
                    };
                    let before = source(&view.data);
                    let fresh = request == view.request;
                    view.arrived(request, result.map(|data| *data));
                    reread = fresh && (before != source(&view.data) || view.rich.is_none());
                }
                if reread {
                    self.load_diff_rich(ctx);
                }
            }
            Msg::DiffFile(request, result) => {
                if let Some(view) = &mut self.diff {
                    view.file_arrived(request, result);
                }
            }
            Msg::DiffRich(request, kind, result) => {
                if let Some(view) = &mut self.diff {
                    view.rich_arrived(request, kind, *result);
                }
            }
            Msg::DiffRendered(request, result) => {
                if let Some(view) = &mut self.diff {
                    view.rendered_arrived(request, *result, ctx);
                }
            }
            Msg::DiffStructural(ids, result) => self.structural_arrived(ids, *result, ctx),
            Msg::PrDetails(request, result) => self.pr_details_arrived(request, *result, ctx),
            Msg::PrFiles(request, result) => self.pr_files_arrived(request, result, ctx),
            Msg::PrPosted { hosting, .. }
                if !super::load::same_repository(Some(&*hosting), self.hosting.as_ref()) => {}
            Msg::PrPosted {
                number,
                what,
                result,
                ..
            } => self.pr_posted(number, what, result, ctx),
            Msg::Compare(request, result) => {
                if let Some(c) = &mut self.compare
                    && c.request == request
                {
                    c.data = Some(result);
                }
            }
            Msg::Mergiraf(path, result) => self.mergiraf_done(path, result),
            Msg::JobPanicked(e) => self.status = Some((e, true)),
            Msg::Then(then) => then(self, ctx),
            Msg::Search(query, code, result) => self.search_done(query, code, result, ctx),
            // Asked before the list asked for last (one read before a merge answering after
            // the one read since), or of the forge the remotes no longer point to.
            Msg::Prs(asked, _) if self.prs_loaded != Some(asked) => {}
            Msg::Prs(_, result) => match result {
                Ok(prs) => {
                    self.prs = prs;
                    self.pr_error = None;
                    if crate::network::background_may_run() {
                        self.refresh_ci(ctx);
                    }
                    self.load_seen_heads(ctx);
                }
                Err(e) => self.pr_error = Some(e),
            },
            Msg::CiStatuses(fresh, result) => self.ci_arrived(fresh, result),
            Msg::CiRuns(request, result) => self.ci_runs_arrived(request, result, ctx),
            Msg::CiJobs(request, run, result) => self.ci_jobs_arrived(request, run, result, ctx),
            Msg::CiLog(request, result) => self.ci_log_arrived(request, result),
            Msg::CiActed(label, result) => self.ci_acted(label, result),
            Msg::OpDone {
                label,
                result,
                refresh,
                kind,
                quiet,
                stopped,
            } => {
                if result.is_err() && !quiet {
                    self.keep_unfinished(stopped);
                }
                self.op_done(label, result, refresh, kind, quiet, ctx)
            }
        }
    }
}

/// Localize finite application-owned operation labels at presentation only.
/// Raw labels remain the identity used by operations, undo, versions and LFS.
/// Unknown provider/custom labels are opaque and are returned unchanged.
pub(crate) fn display_operation(label: &str) -> String {
    operation_display(label, 0)
}

fn operation_display(label: &str, depth: usize) -> String {
    let shared = crate::operation_labels::display(label);
    if shared != label {
        return shared;
    }
    if depth < 4 {
        for (prefix, id) in [
            ("Undo ", "interaction-operation-undo"),
            ("Redo ", "interaction-operation-redo"),
            ("Preparing ", "interaction-operation-preparing"),
            ("Push after ", "interaction-operation-push-after"),
        ] {
            if let Some(inner) = label.strip_prefix(prefix) {
                let rendered = operation_display(inner, depth + 1);
                if rendered != inner {
                    return crate::i18n::message(id, &[("operation", rendered.into())]);
                }
            }
        }
    }
    for (suffix, id) in [
        (" (here only)", "repo-core-operation-dco-here"),
        ("", "repo-core-operation-dco"),
    ] {
        if let Some(branch) = label
            .strip_prefix("Sign off the commits of ")
            .and_then(|v| v.strip_suffix(suffix))
        {
            if !branch.is_empty() {
                return crate::i18n::message(id, &[("branch", branch.into())]);
            }
        }
    }
    use std::sync::OnceLock;
    struct Template {
        pattern: regex::Regex,
        id: &'static str,
        args: &'static [(&'static str, bool)],
    }
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    let templates = TEMPLATES.get_or_init(|| {
        [
            ("(?s)^Inspect\\ shared\\ Git\\ directories\\ and\\ donors;\\ make\\ a\\ clone\\ independent$", "repo-core-0085", &[][..]),
            ("(?s)^Subtrees:\\ register,\\ update,\\ inspect\\ and\\ publish\\ vendored\\ folders…$", "repo-core-0082", &[][..]),
            ("(?s)^Push\\ outbox:\\ queue\\ exact\\ commit\\ or\\ send\\ selected\\ entries…$", "repo-core-0080", &[][..]),
            ("(?s)^Conflicts\\ that\\ can\\ wait:\\ isolated\\ rebase\\ and\\ restack…$", "repo-core-0072", &[][..]),
            ("(?s)^Parallel\\ branches\\ in\\ this\\ folder:\\ hunk\\ ownership…$", "repo-core-0074", &[][..]),
            ("(?s)^Named\\ changelists:\\ messages,\\ commit\\ and\\ shelve…$", "repo-core-0078", &[][..]),
            ("(?s)^Stop\\ remembering\\ conflict\\ resolutions\\ \\(rerere\\)$", "repo-core-0118", &[][..]),
            ("(?s)^Publish\\ submodule\\ dependencies\\ before\\ parent…$", "repo-core-0086", &[][..]),
            ("(?s)^Agent\\ turns:\\ save\\ each\\ turn\\ in\\ this\\ worktree$", "repo-core-0108", &[][..]),
            ("(?s)^Replace\\ refs\\ and\\ temporary\\ history\\ joins…$", "repo-core-0094", &[][..]),
            ("(?s)^Save\\ snapshot\\ \\(stash,\\ keep\\ the\\ changes\\)…$", "repo-core-0048", &[][..]),
            ("(?s)^Remote\\ URLs,\\ refspecs\\ and\\ mirror\\ policy…$", "repo-core-0092", &[][..]),
            ("(?s)^Delete\\ branch\\ locally\\ and\\ on\\ the\\ remote$", "repo-core-3367", &[][..]),
            ("(?s)^Remember\\ conflict\\ resolutions\\ \\(rerere\\)$", "repo-core-0119", &[][..]),
            ("(?s)^Activity:\\ operations\\ and\\ their\\ output$", "repo-core-0066", &[][..]),
            ("(?s)^Download\\ history\\ objects\\ \\(consented\\)$", "repo-core-3615", &[][..]),
            ("(?s)^command\\ output\\ benchmark\\ screenshot$", "repo-core-0039", &[][..]),
            ("(?s)^Inspect\\ state\\ publication\\ receipts…$", "repo-core-0076", &[][..]),
            ("(?s)^Ref\\ storage\\ and\\ reftable\\ migration…$", "repo-core-0096", &[][..]),
            ("(?s)^Working\\-state\\ transaction\\ recovery$", "repo-core-0077", &[][..]),
            ("(?s)^grafts\\ original\\ effective\\ history$", "repo-core-0095", &[][..]),
            ("(?s)^Select\\ stale\\ or\\ merged\\ branches…$", "repo-core-0071", &[][..]),
            ("(?s)^triangular\\ upstream\\ ahead\\ behind$", "repo-core-0089", &[][..]),
            ("(?s)^Push\\ for\\ review,\\ skipping\\ hooks$", "repo-core-0035", &[][..]),
            ("(?s)^publication\\ group\\ retry\\ mirrors$", "repo-core-0091", &[][..]),
            ("(?s)^objects\\ pack\\ delta\\ file\\ version$", "repo-core-0099", &[][..]),
            ("(?s)^All\\ refs\\ and\\ namespace\\ history…$", "repo-core-0101", &[][..]),
            ("(?s)^Pull\\ \\(fast\\-forward\\ if\\ possible\\)$", "repo-core-3093", &[][..]),
            ("(?s)^File\\ versions\\ across\\ branches$", "repo-core-0028", &[][..]),
            ("(?s)^Shared\\ and\\ borrowed\\ objects$", "repo-core-0084", &[][..]),
            ("(?s)^Bake\\-off:\\ compare\\ attempts…$", "repo-core-0107", &[][..]),
            ("(?s)^What\\ a\\ package\\ will\\ publish$", "repo-core-0110", &[][..]),
            ("(?s)^Stash\\ only\\ what\\ is\\ staged…$", "repo-core-0046", &[][..]),
            ("(?s)^Inspect\\ local\\ Git\\ bundle…$", "repo-core-0044", &[][..]),
            ("(?s)^submodules\\ publish\\ remote$", "repo-core-0087", &[][..]),
            ("(?s)^Publish\\ to\\ several\\ hosts…$", "repo-core-0090", &[][..]),
            ("(?s)^Start\\ work\\ from\\ an\\ issue…$", "repo-core-0133", &[][..]),
            ("(?s)^Bring\\ the\\ conflicts\\ back\\?$", "repo-core-0693", &[][..]),
            ("(?s)^Three\\-state\\ staging\\ lens$", "repo-core-0026", &[][..]),
            ("(?s)^Distribute\\ staged\\ fixes…$", "repo-core-0049", &[][..]),
            ("(?s)^fetch\\ push\\ mapping\\ prune$", "repo-core-0093", &[][..]),
            ("(?s)^How\\ an\\ object\\ is\\ stored…$", "repo-core-0098", &[][..]),
            ("(?s)^Pull\\ \\(fast\\-forward\\ only\\)$", "repo-core-3094", &[][..]),
            ("(?s)^Code\\ moved\\ across\\ files$", "repo-core-0022", &[][..]),
            ("(?s)^network\\ offline\\ metered$", "repo-core-0065", &[][..]),
            ("(?s)^Vendored\\ source\\ history$", "repo-core-0083", &[][..]),
            ("(?s)^Forget\\ this\\ resolution\\?$", "repo-core-0688", &[][..]),
            ("(?s)^Worktree\\ dev\\ container…$", "repo-core-4201", &[][..]),
            ("(?s)^Sync\\ \\(pull,\\ then\\ push\\)$", "repo-core-0033", &[][..]),
            ("(?s)^Linked\\ project\\ copies…$", "repo-core-0040", &[][..]),
            ("(?s)^Timeline\\ and\\ undo\\ tree$", "repo-core-0053", &[][..]),
            ("(?s)^graph\\ or\\ visible\\ lines$", "repo-core-4194", &[][..]),
            ("(?s)^Pull\\ and\\ push\\ routes…$", "repo-core-0088", &[][..]),
            ("(?s)^Push\\ reviewed\\ history$", "repo-core-3110", &[][..]),
            ("(?s)^data\\ download\\ remote$", "repo-core-0021", &[][..]),
            ("(?s)^refactor\\ moved\\ block$", "repo-core-0023", &[][..]),
            ("(?s)^First\\ commit\\ created$", "repo-core-0278", &[][..]),
            ("(?s)^Fix\\ outgoing\\ history$", "repo-core-3118", &[][..]),
            ("(?s)^Delete\\ remote\\ branch$", "repo-core-3491", &[][..]),
            ("(?s)^Run\\ Actions\\ locally…$", "repo-core-4197", &[][..]),
            ("(?s)^Behaviour\\ timeline…$", "repo-core-0038", &[][..]),
            ("(?s)^Offline\\ publication$", "repo-core-0081", &[][..]),
            ("(?s)^Release\\ tag\\ created$", "repo-core-0279", &[][..]),
            ("(?s)^Edit\\ commit\\ message$", "repo-core-3172", &[][..]),
            ("(?s)^File\\ migration\\ map$", "repo-core-0024", &[][..]),
            ("(?s)^Export\\ Git\\ bundle…$", "repo-core-0042", &[][..]),
            ("(?s)^refs\\ compatibility$", "repo-core-0097", &[][..]),
            ("(?s)^Interactive\\ rebase$", "repo-core-3337", &[][..]),
            ("(?s)^Stage\\ all\\ changes$", "repo-core-0055", &[][..]),
            ("(?s)^Find\\ lost\\ commits$", "repo-core-0060", &[][..]),
            ("(?s)^Deferred\\ rewrites$", "repo-core-0073", &[][..]),
            ("(?s)^Repository\\ doctor$", "repo-core-0103", &[][..]),
            ("(?s)^Start\\ agent\\ task…$", "repo-core-0105", &[][..]),
            ("(?s)^Update\\ submodules$", "repo-core-0122", &[][..]),
            ("(?s)^Use\\ this\\ decision$", "repo-core-0535", &[][..]),
            ("(?s)^Delete\\ remote\\ tag$", "repo-core-3543", &[][..]),
            ("(?s)^offline\\ exchange$", "repo-core-0045", &[][..]),
            ("(?s)^Virtual\\ branches$", "repo-core-0075", &[][..]),
            ("(?s)^Check\\ CODEOWNERS$", "repo-core-0116", &[][..]),
            ("(?s)^Show\\ CI\\ for\\ HEAD$", "repo-core-0135", &[][..]),
            ("(?s)^Background\\ fetch$", "repo-core-2899", &[][..]),
            ("(?s)^Show\\ that\\ window$", "repo-core-3042", &[][..]),
            ("(?s)^Move\\ commit\\ down$", "repo-core-3175", &[][..]),
            ("(?s)^Toggle\\ terminal$", "repo-core-0061", &[][..]),
            ("(?s)^network\\ offline$", "repo-core-0063", &[][..]),
            ("(?s)^Working\\ changes$", "repo-core-0079", &[][..]),
            ("(?s)^Worktree\\ setup…$", "repo-core-0109", &[][..]),
            ("(?s)^Open\\ in\\ browser$", "repo-core-0115", &[][..]),
            ("(?s)^Merge\\ completed$", "repo-core-0280", &[][..]),
            ("(?s)^Delete\\ new\\ file$", "repo-core-0851", &[][..]),
            ("(?s)^Discard\\ changes$", "repo-core-0856", &[][..]),
            ("(?s)^Remove\\ worktree$", "repo-core-3597", &[][..]),
            ("(?s)^Prune\\ worktrees$", "repo-core-3602", &[][..]),
            ("(?s)^offline\\ backup$", "repo-core-0043", &[][..]),
            ("(?s)^Stash\\ changes…$", "repo-core-0054", &[][..]),
            ("(?s)^Search\\ commits$", "repo-core-0057", &[][..]),
            ("(?s)^refs\\ internals$", "repo-core-0102", &[][..]),
            ("(?s)^Open\\ in\\ editor$", "repo-core-0113", &[][..]),
            ("(?s)^Unity:\\ Set\\-up…$", "repo-core-0124", &[][..]),
            ("(?s)^Finish\\ hotfix…$", "repo-core-0137", &[][..]),
            ("(?s)^Edit\\ the\\ note…$", "repo-core-0547", &[][..]),
            ("(?s)^Move\\ commit\\ up$", "repo-core-3174", &[][..]),
            ("(?s)^Email\\ patches…$", "repo-core-4199", &[][..]),
            ("(?s)^Create\\ branch…$", "repo-core-4203", &[][..]),
            ("(?s)^LFS:\\ Storage…$", "repo-core-0132", &[][..]),
            ("(?s)^Create\\ branch$", "repo-core-0140", &[][..]),
            ("(?s)^Use\\ it\\ anyway$", "repo-core-0534", &[][..]),
            ("(?s)^Stash\\ changes$", "repo-core-1492", &[][..]),
            ("(?s)^Pull\\ \\(rebase\\)$", "repo-core-3095", &[][..]),
            ("(?s)^Delete\\ branch$", "repo-core-3361", &[][..]),
            ("(?s)^Remove\\ remote$", "repo-core-3500", &[][..]),
            ("(?s)^New\\ terminal$", "repo-core-0068", &[][..]),
            ("(?s)^Cut\\ release…$", "repo-core-0139", &[][..]),
            ("(?s)^Add\\ worktree$", "repo-core-1535", &[][..]),
            ("(?s)^Apply\\ patch…$", "repo-core-2630", &[][..]),
            ("(?s)^LFS:\\ Locks…$", "repo-core-0131", &[][..]),
            ("(?s)^Drop\\ commit$", "repo-core-3173", &[][..]),
            ("(?s)^Create\\ tag…$", "repo-core-4204", &[][..]),
            ("(?s)^Go\\ offline$", "repo-core-0064", &[][..]),
            ("(?s)^LFS:\\ Fetch$", "repo-core-0126", &[][..]),
            ("(?s)^Create\\ tag$", "repo-core-0141", &[][..]),
            ("(?s)^Use\\ theirs$", "repo-core-0725", &[][..]),
            ("(?s)^Hard\\ reset$", "repo-core-3355", &[][..]),
            ("(?s)^Delete\\ tag$", "repo-core-3546", &[][..]),
            ("(?s)^Go\\ online$", "repo-core-0062", &[][..]),
            ("(?s)^conflicts$", "repo-core-0120", &[][..]),
            ("(?s)^submodule$", "repo-core-0123", &[][..]),
            ("(?s)^LFS:\\ Pull$", "repo-core-0129", &[][..]),
            ("(?s)^Note\\ why…$", "repo-core-0546", &[][..]),
            ("(?s)^machines$", "repo-core-0041", &[][..]),
            ("(?s)^recovery$", "repo-core-0059", &[][..]),
            ("(?s)^terminal$", "repo-core-0069", &[][..]),
            ("(?s)^settings$", "repo-core-0125", &[][..]),
            ("(?s)^Use\\ mine$", "repo-core-0727", &[][..]),
            ("(?s)^staging$", "repo-core-0027", &[][..]),
            ("(?s)^release$", "repo-core-0138", &[][..]),
            ("(?s)^remote$", "repo-core-0034", &[][..]),
            ("(?s)^absorb$", "repo-core-0050", &[][..]),
            ("(?s)^Reflog$", "repo-core-0058", &[][..]),
            ("(?s)^doctor$", "repo-core-0104", &[][..]),
            ("(?s)^folder$", "repo-core-0112", &[][..]),
            ("(?s)^owners$", "repo-core-0117", &[][..]),
            ("(?s)^builds$", "repo-core-0136", &[][..]),
            ("(?s)^Squash$", "repo-core-2444", &[][..]),
            ("(?s)^Unlock$", "repo-core-3591", &[][..]),
            ("(?s)^Search$", "repo-core-4193", &[][..]),
            ("(?s)^files$", "repo-core-0025", &[][..]),
            ("(?s)^stash$", "repo-core-0047", &[][..]),
            ("(?s)^agent$", "repo-core-0106", &[][..]),
            ("(?s)^issue$", "repo-core-0134", &[][..]),
            ("(?s)^Mixed$", "repo-core-0376", &[][..]),
            ("(?s)^Apply$", "repo-core-0872", &[][..]),
            ("(?s)^above$", "repo-core-2442", &[][..]),
            ("(?s)^below$", "repo-core-2443", &[][..]),
            ("(?s)^Fixup$", "repo-core-2446", &[][..]),
            ("(?s)^Undo$", "repo-core-0051", &[][..]),
            ("(?s)^Redo$", "repo-core-0052", &[][..]),
            ("(?s)^Soft$", "repo-core-0373", &[][..]),
            ("(?s)^Keep$", "repo-core-0380", &[][..]),
            ("(?s)^Hard$", "repo-core-0383", &[][..]),
            ("(?s)^Save$", "repo-core-1062", &[][..]),
            ("(?s)^from$", "repo-core-1101", &[][..]),
            ("(?s)^Move$", "repo-core-2441", &[][..]),
            ("(?s)^into$", "repo-core-2445", &[][..]),
            ("(?s)^Lock$", "repo-core-3593", &[][..]),
            ("(?s)^log$", "repo-core-0067", &[][..]),
            ("(?s)^lfs$", "repo-core-0127", &[][..]),
            ("(?s)^Pop$", "repo-core-0871", &[][..]),
            ("(?s)^Add$", "repo-core-0976", &[][..]),
            ("(?s)^to$", "repo-core-1102", &[][..]),
            ("(?s)^Go\\ to\\ line\\ (.*?)\\ of\\ the\\ file\\ before\\ the\\ change$", "repo-core-2040", &[("o", false)][..]),
            ("(?s)^Remove\\ unused\\ fork\\ remotes\\ \\(([0-9]+)\\)$", "repo-core-0121", &[("unused", true)][..]),
            ("(?s)^Interactive\\ rebase\\ (.*?)\\ onto\\ (.*?)…$", "repo-core-2988", &[("target", false), ("at", false)][..]),
            ("(?s)^Interactive\\ rebase\\ onto\\ (.*?)$", "repo-core-3342", &[("onto-label", false)][..]),
            ("(?s)^Revert\\ a\\ hunk\\ of\\ (.*?)\\ from\\ (.*?)$", "repo-core-3311", &[("path", false), ("hash", false)][..]),
            ("(?s)^Keep\\ both\\ versions\\ of\\ (.*?)$", "repo-core-0590", &[("path", false)][..]),
            ("(?s)^Revert\\ (.*?)\\ as\\ changed\\ in\\ (.*?)$", "repo-core-3309", &[("path", false), ("hash", false)][..]),
            ("(?s)^Revert\\ ([0-9]+)\\ line(.*?)\\ of\\ (.*?)\\ from\\ (.*?)$", "repo-core-3312", &[("n", true), ("", false), ("path", false), ("hash", false)][..]),
            ("(?s)^(.*?)\\ \\(its\\ fixup!\\ commit\\)$", "repo-core-2538", &[("label", false)][..]),
            ("(?s)^Rebase\\ commit\\ (.*?),\\ (.*?),\\ (.*?),\\ (.*?)$", "repo-core-3249", &[("hash", false), ("summary", false), ("author", false), ("label", false)][..]),
            ("(?s)^Edit\\ the\\ message\\ of\\ (.*?)$", "repo-core-3321", &[("hash", false)][..]),
            ("(?s)^Create\\ patch\\ from\\ (.*?)…$", "repo-core-2994", &[("at", false)][..]),
            ("(?s)^Commit\\ fixup!\\ for\\ (.*?)$", "repo-core-2529", &[("hash", false)][..]),
            ("(?s)^Create\\ branch\\ at\\ (.*?)…$", "repo-core-2990", &[("at", false)][..]),
            ("(?s)^Export\\ bundle\\ to\\ (.*?)$", "repo-core-0267", &[("file", false)][..]),
            ("(?s)^Update\\ submodule\\ (.*?)$", "repo-core-2844", &[("path", false)][..]),
            ("(?s)^Reset\\ (.*?)\\ to\\ here…$", "repo-core-2986", &[("target", false)][..]),
            ("(?s)^Create\\ tag\\ at\\ (.*?)…$", "repo-core-2992", &[("at", false)][..]),
            ("(?s)^Open\\ (.*?)\\ in\\ editor$", "repo-core-3001", &[("path", false)][..]),
            ("(?s)^([0-9]+)\\ picked\\ file(.*?)$", "repo-core-1498", &[("n", true), ("", false)][..]),
            ("(?s)^(.*?)\\ notes\\ “(.*?)”\\ (.*?)\\ (.*?)$", "repo-core-1100", &[("Push", false), ("namespace", false), ("", false), ("remote", false)][..]),
            ("(?s)^Rename\\ (.*?)\\ to\\ (.*?)$", "repo-core-1233", &[("old", false), ("new", false)][..]),
            ("(?s)^Go\\ to\\ line\\ ([0-9]+)$", "repo-core-2039", &[("n", true)][..]),
            ("(?s)^(Stage|Unstage|Discard|Revert)\\ ([0-9]+)\\ line(.*?)\\ of\\ (.*?)$", "repo-core-0164", &[("verb", false), ("n", true), ("", false), ("path", false)][..]),
            ("(?s)^Fold\\ into\\ (.*?)$", "repo-core-2530", &[("hash", false)][..]),
            ("(?s)^(.*?)\\ worktree\\ (.*?)$", "repo-core-3595", &[("label", false), ("name", false)][..]),
            ("(?s)^Push\\ tag\\ (.*?)$", "repo-core-3541", &[("name", false)][..]),
            ("(?s)^Doctor:\\ (.*?)$", "repo-core-0114", &[("title", false)][..]),
            ("(?s)^Unstage\\ (.*?)$", "repo-core-3010", &[("path", false)][..]),
            ("(?s)^Create\\ (pull request|merge request)$", "repo-core-0360", &[("noun", false)][..]),
            ("(?s)^Remote\\ (.*?)$", "repo-core-1468", &[("name", false)][..]),
            ("(?s)^(.*?):\\ done$", "repo-core-0269", &[("label", false)][..]),
            ("(?s)^(Soft|Mixed|Keep|Hard)\\ reset$", "repo-core-0387", &[("label", false)][..]),
            ("(?s)^Track\\ (.*?)$", "repo-core-1532", &[("upstream", false)][..]),
            ("(?s)^(Soft|Mixed|Keep|Hard)\\ reset$", "repo-core-2659", &[("label", false)][..]),
            ("(?s)^Stage\\ (.*?)$", "repo-core-3007", &[("path", false)][..]),
            ("(?s)^Fetch\\ (.*?)$", "repo-core-3495", &[("name", false)][..]),
            ("(?s)^(.*?)\\ for\\ (.*?)$", "repo-core-0729", &[("label", false), ("path", false)][..]),
            ("(?s)^Open\\ (.*?)$", "repo-core-3003", &[("path", false)][..]),
            ("(?s)^Pop\\ (.*?)$", "repo-core-3512", &[("name", false)][..]),
            ("(?s)^(Move|Squash|Fixup)\\ (.*?)\\ (above|below|into)\\ (.*?)$", "repo-core-2447", &[("verb", false), ("hash", false), ("word", false), ("target", false)][..]),
            ("(?s)^(.*?)\\ (.*?)$", "repo-core-2999", &[("label", false), ("path", false)][..]),
        ].into_iter().map(|(pattern, id, args)| Template {
            pattern: regex::Regex::new(pattern).expect("fixed operation label pattern"),
            id, args,
        }).collect()
    });
    if depth < 4 {
        for template in templates {
            let Some(captures) = template.pattern.captures(label) else {
                continue;
            };
            // A template whose leading value is another operation must recognize it.
            // This prevents generic label/path forms from translating custom commands.
            if !matches!(
                template.id,
                "repo-core-0164" | "repo-core-2447" | "repo-core-0387" | "repo-core-2659"
            ) && template
                .args
                .first()
                .is_some_and(|(name, _)| matches!(*name, "label" | "verb" | "Push"))
            {
                let inner = captures.get(1).map_or("", |v| v.as_str());
                if crate::i18n::language() == crate::i18n::Language::German
                    && operation_display(inner, depth + 1) == inner
                {
                    continue;
                }
            }
            let args: Vec<(&str, crate::i18n::Value<'_>)> = template
                .args
                .iter()
                .enumerate()
                .filter(|(_, (name, _))| !name.is_empty())
                .map(|(i, (name, numeric))| {
                    let raw = captures.get(i + 1).map_or("", |v| v.as_str());
                    let value = if *numeric {
                        raw.parse::<f64>()
                            .map(Into::into)
                            .unwrap_or_else(|_| raw.into())
                    } else if !matches!(
                        template.id,
                        "repo-core-0164" | "repo-core-2447" | "repo-core-1100"
                    ) && matches!(*name, "label" | "verb" | "what" | "to-lowercase")
                    {
                        operation_display(raw, depth + 1).into()
                    } else {
                        raw.into()
                    };
                    (*name, value)
                })
                .collect();
            return crate::i18n::message(template.id, &args);
        }
    }
    label.to_owned()
}
/// Present a status without changing its stored operation/status identity.
pub(crate) fn display_status(status: &str) -> String {
    if let Some(rest) = status.strip_prefix(crate::git::CANCELLED) {
        const PULL_NOTE: &str = "\nIf the pull had started merging or rebasing, that is left in progress: Abort in the toolbar goes back to where the branch was.";
        let (body, pull) = rest
            .strip_suffix(PULL_NOTE)
            .map_or((rest, false), |body| (body, true));
        let rendered = if body.is_empty() {
            Some(crate::i18n::text("repo-core-cancelled").to_owned())
        } else {
            [(
                "\nUntil it is deleted, git refuses to work in this repository. Delete it if no other git program is running here.", "one"
            ), (
                "\nUntil them are deleted, git refuses to work in this repository. Delete them if no other git program is running here.", "many"
            )].into_iter().find_map(|(suffix, amount)| {
                body.strip_prefix(", but git left ").and_then(|v| v.strip_suffix(suffix)).and_then(|v| v.strip_suffix(" behind"))
                    .map(|locks| crate::i18n::message("repo-core-cancelled-locks", &[("locks", locks.into()), ("amount", amount.into())]))
            })
        };
        if let Some(mut rendered) = rendered {
            if pull {
                rendered.push_str(crate::i18n::text("repo-core-4160"));
            }
            return rendered;
        }
    }
    let rendered = display_operation(status);
    if rendered != status {
        return rendered;
    }
    match status.split_once(": ") {
        Some((label, detail)) => format!(
            "{}: {}",
            display_operation(label),
            crate::git::display_error(&crate::forge::inbox::display_error(detail))
        ),
        None => crate::git::display_error(&crate::forge::inbox::display_error(status)),
    }
}

#[cfg(test)]
mod fluent_presentation_tests {
    use super::*;

    #[test]
    fn repository_operation_paint_preserves_control_labels_and_opaque_arguments() {
        let path = "src/Commit 日本語.rs";
        let raw = format!("Stage 1234 lines of {path}");
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            // Unknown external text forces all finite regexes to initialize, even when
            // a known German label could return early through the shared adapter.
            assert_eq!(
                display_operation("external-custom --flag=literal"),
                "external-custom --flag=literal"
            );
            let german = language == "de";
            assert_eq!(
                display_operation("Remove unused fork remotes (1234)"),
                if german {
                    "Unbenutzte Fork-Remotes entfernen (1.234)"
                } else {
                    "Remove unused fork remotes (1,234)"
                }
            );
            assert_eq!(
                display_operation("Abort rebase"),
                if german {
                    "Rebase abbrechen"
                } else {
                    "Abort rebase"
                }
            );
            assert_eq!(
                display_operation("Continue merge"),
                if german {
                    "Merge fortsetzen"
                } else {
                    "Continue merge"
                }
            );
            let rendered = display_operation(&raw);
            assert_eq!(
                rendered,
                if german {
                    format!("1.234 Zeilen von {path} bereitstellen")
                } else {
                    format!("Stage 1,234 lines of {path}")
                }
            );
            assert_eq!(
                display_operation("Stage 1 line of x"),
                if german {
                    "1 Zeile von x bereitstellen"
                } else {
                    "Stage 1 line of x"
                }
            );
            assert_eq!(
                display_operation("Fetch notes “Commit” from origin"),
                if german {
                    "Notizen „Commit“ von origin abrufen"
                } else {
                    "Fetch notes “Commit” from origin"
                }
            );
            assert_eq!(
                display_operation("Move aaaaaaaa above bbbbbbbb"),
                if german {
                    "aaaaaaaa vor bbbbbbbb verschieben"
                } else {
                    "Move aaaaaaaa above bbbbbbbb"
                }
            );
            assert_eq!(
                display_operation("Undo Sign off the commits of Commit/ü"),
                if german {
                    "Rückgängig: Commits von Commit/ü mit Signed-off-by versehen"
                } else {
                    "Undo Sign off the commits of Commit/ü"
                }
            );
            for custom in [
                "deploy --target=de",
                "Abort deployment",
                "Continue deployment",
                "Undo deployment script",
                "custom command with arguments",
                "外部コマンド",
            ] {
                assert_eq!(display_operation(custom), custom);
                assert_eq!(display_status(custom), custom);
            }
            assert!(raw.starts_with("Stage "));
            assert_eq!(raw, format!("Stage 1234 lines of {path}"));
            assert_eq!(
                crate::repo::pull::PullMode::FastForwardOnly.args(),
                &["pull", "--ff-only"]
            );
        }
        crate::i18n::set_language("xx-unsupported");
        assert_eq!(display_operation("Abort rebase"), "Abort rebase");
        assert_eq!(
            crate::i18n::text("repo-core-nonexistent-fallback-test"),
            "repo-core-nonexistent-fallback-test"
        );
    }

    #[test]
    fn repository_status_paint_localizes_only_known_errors_and_preserves_raw_state() {
        crate::i18n::set_language("de");
        let raw = "Analysis cancelled";
        assert_eq!(display_status(raw), crate::git::display_error(raw));
        assert_ne!(display_status(raw), raw);
        assert_eq!(
            display_status("fatal: external provider message in English"),
            "fatal: external provider message in English"
        );
        assert_eq!(display_status(crate::git::CANCELLED), "Abgebrochen");
        assert_eq!(raw, "Analysis cancelled");
        crate::i18n::set_language("en-US");
        assert_eq!(display_status(crate::git::CANCELLED), crate::git::CANCELLED);
    }
}
