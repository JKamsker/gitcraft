//! Reading the repository in the background and rebuilding the graph rows from what was read.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::branch_visibility::Visibility;
use super::{DiffKey, DiffView, MAX_HISTORY, Msg, RepoTab, WIP_HASH, spawn};
use crate::forge::{self, Hosting};
use crate::git::{self, Commit, Identity, Refs, Remote, Stash, Submodule, WorkStatus, Worktree};
use crate::graph;
use crate::settings::Settings;

/// How often the other worktrees of a repository elsewhere are counted again, at most (see
/// [`RepoTab::recount_siblings`]).
const RECOUNT_EVERY: Duration = Duration::from_secs(30);

pub(super) struct Loaded {
    pub(super) refs: Refs,
    pub(super) history: Vec<Arc<Commit>>,
    pub(super) history_namespaces: Vec<String>,
    pub(super) excluded_namespace_count: Option<usize>,
    pub(super) work: WorkStatus,
    pub(super) worktrees: Vec<Worktree>,
    pub(super) worktree_root: String,
    pub(super) stashes: Vec<Stash>,
    pub(super) remotes: Vec<Remote>,
    /// Usable forge remotes; the latest UI preference chooses among these on arrival.
    pub(super) forge_remotes: Vec<(String, Hosting)>,
    pub(super) forge_generation: u64,
    pub(super) gerrit: Option<crate::gerrit::Server>,
    pub(super) submodules: Vec<Submodule>,
    /// It keeps files in Git LFS, and git-lfs is installed.
    pub(super) lfs: bool,
    pub(super) practice: Option<crate::practice::Session>,
    pub(super) identity: Identity,
    /// How many commits were asked for (more exist when that many came back).
    pub(super) requested: usize,
    /// What `history` was logged from.
    pub(super) log_from: Option<Vec<String>>,
    /// The order `history` was read in.
    pub(super) order: git::LogOrder,
    /// Which read this is (see `RepoTab::reads`).
    pub(super) read: u64,
    /// The issues the branches were made for, and the remotes' default branches.
    pub(super) issues: super::issue_work::Read,
}

/// Whether two hostings name the same web instance and repository, however Git reaches it
/// (HTTPS or SSH): the first load, with nothing before it, moves nothing anywhere.
pub(super) fn same_repository(before: Option<&Hosting>, now: Option<&Hosting>) -> bool {
    match (before, now) {
        (None, _) => true,
        (Some(a), Some(b)) => a.same_repository(b),
        (Some(_), None) => false,
    }
}

impl RepoTab {
    /// Re-reads refs, history, working tree, worktrees, stashes, remotes and submodules in the
    /// background.
    pub fn reload(&mut self, ctx: &egui::Context) {
        self.work_epoch = self.work_epoch.wrapping_add(1);

        self.saved_views.cancel();
        // Not read yet, or suspended: woken as when shown, which watches and reads once.
        if self.sleeping() {
            self.start_deferred(ctx);
            return;
        }
        // Changes seen on disk so far are read by this reload (or the one queued after the
        // current one): reloading for them again would read the same.
        self.fs_pending = None;
        // What merges couldn't be redone for (a machine not reached…) is tried again.
        self.remerges.forget_failures();
        if self.loading {
            // What changed may have been read before it changed: read again once this is in.
            self.reload_again = true;
            return;
        }
        self.loading = true;
        self.load_started = Some(Instant::now());
        let read = self.next_read();
        let forge_generation = self.forge_generation;
        let path = self.path.clone();
        // Keep the pages already loaded.
        let count = self.history.len().clamp(git::LOG_PAGE, MAX_HISTORY);
        let mut visibility = Visibility::load(ctx, self.repo_key());
        let order = super::graph_order::load(ctx, self.repo_key());
        let excluded = super::history_filter::rules(ctx, self.repo_key()).excludes();
        // Answered all the same if it panics: the tab reloads only once it is.
        let failed = |e| Msg::Loaded(Err(e));
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            Msg::Loaded((|| {
                let refs = git::refs(&path)?;
                visibility.prune(&refs);
                let log_from = visibility.revisions(&refs);
                // Leaf reads share one fixed pool across every tab. The parent waits in
                // the job pool; no reload creates its own scoped OS threads.
                let history = {
                    let (path, from, excluded) = (path.clone(), log_from.clone(), excluded.clone());
                    crate::tasks::read(move || {
                        git::log_scoped(&path, from.as_deref(), order, 0, count, &excluded)
                    })?
                };
                let work_trees = {
                    let path = path.clone();
                    crate::tasks::read(move || {
                        let work = git::status(&path)?;
                        let changes = Some(work.changed_files());
                        let worktrees = if crate::remote::is_remote(&path) {
                            git::worktree_list(&path, changes)
                        } else {
                            git::worktrees(&path, changes)
                        }
                        .unwrap_or_default();
                        Ok::<_, String>((work, worktrees))
                    })?
                };
                let stashes = {
                    let path = path.clone();
                    crate::tasks::read(move || git::stashes(&path).unwrap_or_default())?
                };
                let remotes = {
                    let path = path.clone();
                    crate::tasks::read(move || git::remotes(&path))?
                };
                let submodules = {
                    let path = path.clone();
                    crate::tasks::read(move || git::submodules(&path).unwrap_or_default())?
                };
                let lfs = {
                    let path = path.clone();
                    crate::tasks::read(move || git::uses_lfs(&path) && git::lfs_installed(&path))?
                };
                let issues = {
                    let path = path.clone();
                    crate::tasks::read(move || super::issue_work::read(&path))?
                };
                let excluded_namespace_count = git::history_scope::excluded_namespace_count(
                    &path,
                    log_from.as_deref(),
                    &excluded,
                )
                .ok();
                let identity = git::identity(&path);
                let worktree_root = git::read_worktree_root(&path).map_err(|e| {
                    crate::i18n::message("repo-core-2893", &[("e", e.to_string().into())])
                })?;
                let history = history.join()??.into_iter().map(Arc::new).collect();
                let (work, worktrees) = work_trees.join()??;
                let (stashes, remotes, submodules, lfs, issues) = (
                    stashes.join()?,
                    remotes.join()??,
                    submodules.join()?,
                    lfs.join()?,
                    issues.join()?,
                );
                let path = path.as_path();
                let gerrit = crate::gerrit::detect(path, &remotes);
                let forge_remotes = forge::detect_remotes(path, &remotes);
                Ok(Box::new(Loaded {
                    refs,
                    history,
                    history_namespaces: excluded,
                    excluded_namespace_count,
                    work,
                    worktrees,
                    worktree_root,
                    stashes,
                    remotes,
                    forge_remotes,
                    forge_generation,
                    gerrit,
                    submodules,
                    lfs,
                    practice: crate::practice::load(path),
                    identity,
                    requested: count,
                    log_from,
                    order,
                    read,
                    issues,
                }))
            })())
        });
        // The merges put aside for an agent: rows of the graph of their own.
        self.read_parked(ctx);
    }

    pub(super) fn reload_status(&mut self, ctx: &egui::Context) {
        self.work_epoch = self.work_epoch.wrapping_add(1);
        let read = self.next_read();
        self.work_pending = read;
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || Msg::Status(read, git::status(&path)));
    }

    /// The watcher is there now. What changed while it was being set up (it walks the whole
    /// worktree, which takes a while) made no event: whatever was read before may have missed
    /// it. Not for a first read yet to start (it reads after this), nor for a parked view
    /// (it reloads when shown).
    pub(super) fn watcher_arrived(&mut self, ctx: &egui::Context) {
        if self.sleeping() || !self.shown {
            return;
        }
        if self.loading {
            self.recheck_after_load = true;
        } else {
            self.recheck(ctx);
        }
    }

    /// Counts, in the background, the changes of the worktrees the last read left uncounted (on
    /// another machine, where that takes a while), one count at a time.
    /// Every count is made again (those shown are the last ones, kept until then); a read
    /// coming in while one runs counts again after it.
    fn count_worktrees(&mut self, ctx: &egui::Context) {
        if !crate::remote::is_remote(&self.path) {
            return;
        }
        if self.counting_worktrees {
            self.recount_worktrees = true;
            return;
        }
        let mut list = self.worktrees.clone();
        for wt in list.iter_mut().filter(|w| !w.current) {
            wt.changes = None;
        }
        if !list.iter().any(git::Worktree::needs_count) {
            return;
        }
        self.counting_worktrees = true;
        self.counted_at = Instant::now();
        let path = self.path.clone();
        // Answered with no counts should it fail, so that counting isn't held up for good.
        let failed = |_| Msg::WorktreeCounts(Vec::new());
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            git::count_changes(&path, &mut list);
            Msg::WorktreeCounts(
                list.into_iter()
                    .filter_map(|w| Some((w.path, w.changes?)))
                    .collect(),
            )
        });
    }

    /// Counts the other worktrees of a repository elsewhere again every so often while the
    /// window has the focus: the poller looks only at this one, so a change made in another
    /// (an editor there) would otherwise show only on the next reload.
    pub(super) fn recount_siblings(&mut self, ctx: &egui::Context) {
        let others = self
            .worktrees
            .iter()
            .any(|w| !w.current && !w.bare && w.prunable.is_none());
        if !crate::remote::is_remote(&self.path)
            || !others
            || self.loading
            || self.counting_worktrees
            || !ctx.input(|i| i.focused)
        {
            return;
        }
        let since = self.counted_at.elapsed();
        if since < RECOUNT_EVERY {
            ctx.request_repaint_after(RECOUNT_EVERY - since);
            return;
        }
        self.count_worktrees(ctx);
    }

    /// Takes in the counts of [`Self::count_worktrees`].
    pub(super) fn worktrees_counted(&mut self, counts: Vec<(PathBuf, usize)>, ctx: &egui::Context) {
        self.counting_worktrees = false;
        for wt in self.worktrees.iter_mut().filter(|w| !w.current) {
            if let Some((_, n)) = counts.iter().find(|(p, _)| *p == wt.path) {
                wt.changes = Some(*n);
            }
        }
        if std::mem::take(&mut self.recount_worktrees) {
            self.count_worktrees(ctx);
        }
    }

    /// Reads the working tree and submodules again, and the refs to see whether anything
    /// moved: rather than a whole reload, which on a large repository is far longer.
    fn recheck(&mut self, ctx: &egui::Context) {
        self.reload_status(ctx);
        self.reload_submodules(ctx);
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || Msg::RefsCheck(git::refs(&path)));
    }

    /// The number of a read starting now.
    fn next_read(&mut self) -> u64 {
        self.discard_decrypted();
        if let Some(state) = &mut self.file_comparison {
            state.invalidate();
        }
        self.reads += 1;
        self.reads
    }

    /// Whether an answer of `read` is newer than what is shown from the read `shown`; if so,
    /// it becomes what is shown.
    pub(super) fn newer(read: u64, shown: &mut u64) -> bool {
        let newer = read > *shown;
        if newer {
            *shown = read;
        }
        newer
    }

    /// Editing transport identity invalidates credentials and account-dependent read results.
    /// Already submitted operations keep their immutable original context.
    pub(crate) fn notice_connections(&mut self, ctx: &egui::Context) {
        let now = crate::connections::current();
        if self.connection_context == now {
            return;
        }
        self.connection_context = now;
        self.invalidate_https_challenge();
        self.credentials = None;
        if matches!(self.dialog, Some(super::Dialog::Credentials { .. })) {
            self.dialog = None;
        }
        self.prs.clear();
        self.pr_error = None;
        self.prs_loaded = Some(Instant::now());
        self.forget_merged_prs();
        self.ci.account_changed();
        self.ci_view = None;
        self.avatars_hinted = false;
        self.pr_requests += 1;
        self.invalidate_pr_document();
        if let Some(view) = &mut self.pr_view {
            view.account_changed(self.pr_requests);
        }
        for parked in &mut self.parked {
            parked.notice_connections(ctx);
        }
        if self.sleeping() {
            return;
        }
        self.reload_identity(ctx);
        if crate::network::background_may_run() {
            self.load_prs(ctx);
            self.refresh_ci(ctx);
            if self.pr_view.is_some() {
                self.load_pr(ctx);
            }
            self.hint_avatars(ctx);
        }
    }

    /// Re-reads the submodules: the commits they should be at are in the index, which can
    /// change without any ref moving (`git add` of a submodule, `git submodule add`).
    /// Reads again who commits are made as: the profile's author changed.
    pub fn reload_identity(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || Msg::Identity(git::identity(&path)));
    }

    pub(super) fn reload_submodules(&mut self, ctx: &egui::Context) {
        let read = self.next_read();
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            Msg::Submodules(read, git::submodules(&path).unwrap_or_default())
        });
    }

    pub(super) fn load_more(&mut self, ctx: &egui::Context) {
        if self.saved_views.active() {
            self.load_graph_view_page();
            return;
        }
        if self.loading_more || !self.has_more || self.history.len() >= MAX_HISTORY {
            return;
        }
        self.loading_more = true;
        let path = self.path.clone();
        let skip = self.history.len();
        let generation = self.history_gen;
        let from = self.log_from.clone();
        let order = self.log_order;
        let excluded = self
            .history_filter
            .read_namespaces
            .clone()
            .unwrap_or_else(git::history_scope::default_hidden_namespaces);
        let stream = Arc::clone(&self.log_stream);
        // Answered all the same if it panics: no more is read until it is.
        let failed_excluded = excluded.clone();
        let failed = move |e| Msg::More(generation, failed_excluded, Err(e));
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            let page = (|| {
                let mut slot = stream
                    .lock()
                    .map_err(|_| crate::i18n::text("repo-core-2894").to_owned())?;
                // The first page of this history (since it was last reloaded) starts the
                // `git log` the next ones are read from.
                if slot
                    .as_ref()
                    .is_none_or(|(made_for, _)| *made_for != generation)
                {
                    *slot = None;
                    *slot = Some((
                        generation,
                        git::LogStream::start_scoped(
                            &path,
                            from.as_deref(),
                            order,
                            skip,
                            &excluded,
                        )?,
                    ));
                }
                let page = match slot.as_mut() {
                    Some((_, log)) => log.next_page(git::LOG_PAGE),
                    None => Ok(Vec::new()),
                };
                // At the end of history (or after an error): nothing more to read from it.
                if !matches!(&page, Ok(p) if p.len() == git::LOG_PAGE) {
                    *slot = None;
                }
                page
            })();
            Msg::More(generation, excluded, page)
        });
    }

    pub(super) fn load_prs(&mut self, ctx: &egui::Context) {
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let asked = Instant::now();
        self.prs_loaded = Some(asked);
        let path = self.path.clone();
        spawn(&self.forge_sender(), ctx, move || {
            Msg::Prs(asked, hosting.forge.provider().list(&path, &hosting))
        });
        self.load_merged_prs(ctx);
    }

    /// Takes in what `reload` read, then reads again if a reload was asked for meanwhile.
    pub(super) fn loaded(&mut self, result: Result<Box<Loaded>, String>, ctx: &egui::Context) {
        self.loading = false;
        let resumed = std::mem::take(&mut self.resuming);
        if let Some(started) = self.load_started.take() {
            if self.load_times.len() == 5 {
                self.load_times.pop_front();
            }
            self.load_times.push_back(started.elapsed());
        }
        match result {
            Ok(loaded) => {
                self.take_loaded(*loaded, ctx);
                self.read_custom_commands(ctx);
                self.versions_after_load(ctx);
                if self.rerere.setup.is_none() {
                    self.read_rerere(ctx);
                }
            }
            Err(e) => {
                self.explain_failure(&e, super::error_help::RELOAD, ctx);
                self.load_error = Some(e);
            }
        }
        if resumed {
            self.reopen_after_resume(ctx);
        }
        if std::mem::take(&mut self.reload_again) {
            self.reload(ctx);
        } else if std::mem::take(&mut self.recheck_after_load) {
            self.recheck(ctx);
        }
    }

    fn take_loaded(&mut self, loaded: Loaded, ctx: &egui::Context) {
        self.load_error = None;
        self.invalidate_mutable_file_view(ctx);
        // A page asked for before this was read past the old history, which
        // this one replaces: it would leave a gap.
        self.history_gen += 1;
        // Who wrote what and which files there are may have changed: read again when next
        // suggested.
        self.search.suggest.forget_sources();
        // The `git log` older pages came from read the history replaced: let it go (once a
        // page being read from it is in).
        self.log_stream = Default::default();
        self.loading_more = false;
        let refs_moved = loaded.refs != self.refs;
        self.refs = loaded.refs;
        self.graph_badge_index = super::graph_view::BadgeIndex::new(&self.refs);
        self.has_more = loaded.history.len() >= loaded.requested;
        let namespace_changed = self
            .history_filter
            .read_namespaces
            .as_ref()
            .is_some_and(|old| *old != loaded.history_namespaces);
        let selected = self.selected.and_then(|i| self.commits.get(i));
        let pinned = selected
            .filter(|c| {
                c.hash != WIP_HASH
                    && (namespace_changed
                        || self
                            .history_filter
                            .pinned
                            .as_ref()
                            .is_some_and(|p| p.hash == c.hash))
                    && !loaded.history.iter().any(|h| h.hash == c.hash)
            })
            .cloned();
        self.history_filter.pinned_layout = pinned.as_ref().map(|commit| {
            let source = self
                .history_filter
                .pinned_layout
                .as_ref()
                .filter(|p| p.hash == commit.hash)
                .unwrap_or(commit);
            super::history_filter::pinned_projection(source, &self.history, &loaded.history)
        });
        self.history_filter.pinned = pinned;
        self.history = loaded.history;
        self.history_filter.read_namespaces = Some(loaded.history_namespaces);
        self.history_filter.excluded_total = loaded.excluded_namespace_count;
        let filters = super::history_filter::rules(ctx, self.repo_key());
        if self.history_filter.read_namespaces.as_ref() != Some(&filters.excludes()) {
            self.reload_again = true;
        }
        self.history_filter.applied = Some(filters);
        // A status read since this reload started is newer.
        let state = loaded.work.in_progress;
        if Self::newer(loaded.read, &mut self.work_read) {
            if loaded.read >= self.work_pending {
                self.work_pending = 0;
            }
            self.work = loaded.work;
            self.stash_picks.keep_changed(&self.work);
        }
        self.refresh_detached(ctx);
        self.folding
            .saw_status(&self.work, &self.refs.head, std::time::Instant::now());
        // The last counts of worktrees this read left uncounted (on another machine) stay
        // until they are counted again.
        let counted: Vec<(PathBuf, Option<usize>)> = self
            .worktrees
            .iter()
            .map(|w| (w.path.clone(), w.changes))
            .collect();
        self.take_worktree_identity(loaded.worktrees, loaded.worktree_root);
        self.sync_branch_bands(ctx);
        self.timeline_read(loaded.read, ctx);
        self.celebration_loaded(loaded.read, ctx);
        self.fold_loaded(loaded.read, state, ctx);
        let remote = crate::remote::is_remote(&self.path);
        for wt in self
            .worktrees
            .iter_mut()
            .filter(|w| remote && w.needs_count())
        {
            if let Some((_, changes)) = counted.iter().find(|(p, _)| *p == wt.path) {
                wt.changes = *changes;
            }
        }
        // This worktree's count is that of the working tree kept, which may be newer.
        let changes = self.work.changed_files();
        if let Some(here) = self.worktrees.iter_mut().find(|wt| wt.current) {
            here.changes = Some(changes);
        }
        self.count_worktrees(ctx);
        self.check_fence(ctx);
        self.read_stacks(ctx);
        self.load_authorship(ctx);
        self.take_visibility(loaded.log_from, ctx);
        self.take_order(loaded.order, ctx);
        // Stash numbers shift when one is pushed or dropped; follow the commit.
        let viewed = self
            .stash_view
            .and_then(|i| self.stashes.get(i))
            .map(|s| s.hash.clone());
        self.stashes = loaded.stashes;
        self.stash_view = viewed.and_then(|h| self.stashes.iter().position(|s| s.hash == h));
        self.identity = loaded.identity;
        if Self::newer(loaded.read, &mut self.submodules_read) {
            self.submodules = loaded.submodules;
        }
        self.lfs = loaded.lfs;
        self.practice = loaded.practice;
        self.practice_result = None;
        self.take_issue_read(loaded.issues);
        let remotes_changed = loaded.remotes != self.remotes;
        if remotes_changed {
            self.invalidate_https_challenge();
        }
        self.remotes = loaded.remotes;
        self.forge_remotes = loaded.forge_remotes;
        self.take_forge_catalog(remotes_changed, loaded.forge_generation, ctx);
        self.gerrit = loaded.gerrit;
        // The forge is asked by itself only while gitcraft may go on the network by itself.
        let paused = !crate::network::background_may_run();
        if !paused
            && self
                .prs_loaded
                .is_none_or(|t| t.elapsed() > Duration::from_secs(120))
        {
            self.load_prs(ctx);
        }
        if !paused {
            self.refresh_ci(ctx);
        }
        self.want_health(ctx);
        self.rebuild(ctx);
        self.graph_motion_loaded(loaded.read, ctx);
        // After the selection is settled: not for a commit that went (amended, rebased).
        self.extras_after_reload(refs_moved, ctx);
        self.hint_avatars(ctx);
        self.reload_compare(ctx);
        self.reflog_after_reload(ctx);
        // A bisect started or moved on in a terminal shows too.
        self.read_bisect(ctx);
        self.clear_parallel_leftovers();
        self.read_test_lane(ctx);
    }

    /// Discovery refines repository identity; mutation scheduling remains separate.
    pub(super) fn take_worktree_identity(&mut self, worktrees: Vec<Worktree>, root: String) {
        self.worktrees = worktrees;
        self.worktree_root = root;
        if crate::remote::is_remote(&self.path) {
            self.tx.repo = self.repo_key().to_owned();
        }
    }

    /// Appends a page of older history that `load_more` read.
    pub(super) fn more_loaded(&mut self, result: Result<Vec<Commit>, String>, ctx: &egui::Context) {
        self.loading_more = false;
        match result {
            Ok(page) => {
                self.has_more = page.len() == git::LOG_PAGE;
                // A reload in between may have loaded these already.
                let known: HashSet<&str> = self.history.iter().map(|c| c.hash.as_str()).collect();
                let fresh: Vec<Arc<Commit>> = page
                    .into_iter()
                    .filter(|c| !known.contains(c.hash.as_str()))
                    .map(Arc::new)
                    .collect();
                self.history.extend(fresh);
                self.rebuild(ctx);
            }
            Err(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
        }
    }

    /// Rebuilds the graph rows from `history` and `work`, keeping the selection.
    pub(super) fn rebuild(&mut self, ctx: &egui::Context) {
        // Asleep, or woken and not read again: there is no history to lay out.
        if self.sleeping() || self.resuming {
            return;
        }
        let previous = self.selected_hash();
        self.rebuild_rows();
        self.saved_views.dirty = true;
        self.restore_selection(previous, ctx);
        self.take_prepared_message();
        self.refresh_work_diff(ctx);
        if let Some(hash) = self.pending_goto.clone() {
            self.goto(&hash, ctx);
        }
    }

    /// After a status read that leaves the rows as they were (the changes row neither appeared
    /// nor went away): what `rebuild` does besides laying out the graph again.
    pub(super) fn refresh_work(&mut self, ctx: &egui::Context) {
        if let Some(wip) = self.commits.first_mut()
            && wip.hash == WIP_HASH
        {
            Arc::make_mut(wip).time = git::now();
        }
        self.take_prepared_message();
        self.refresh_work_diff(ctx);
    }

    /// The WIP pseudo-commit (when the tree is dirty), then `history` with its stashes, laid out
    /// as the graph.
    pub(super) fn rebuild_rows(&mut self) {
        self.saved_views.dirty = true;
        self.history_index = self
            .history
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let selected_hash = self.selected_hash();
        let mut protected = self.history_filter.revealed.clone();
        protected.extend(self.selected_hash());
        protected.extend(self.pending_goto.clone());
        protected.extend(self.search.matches.iter().cloned());
        protected.insert(self.refs.head.clone());
        protected.extend(self.stashes.iter().map(|s| s.base.clone()));
        let rules = self.history_filter.applied.clone().unwrap_or_default();
        let (visible, graph_history, hidden, kept) = super::history_filter::compact(
            &self.history,
            &rules,
            &protected,
            self.log_order.first_parent,
            self.has_more,
        );
        self.history_filter.hidden_loaded = hidden;
        self.history_filter.kept_loaded = kept;
        let old_rows = std::mem::take(&mut self.commits);
        if !self.work.is_clean() {
            self.commits.push(Arc::new(Commit {
                hash: WIP_HASH.to_owned(),
                parents: if self.refs.head.is_empty() {
                    Vec::new()
                } else {
                    vec![self.refs.head.clone()]
                },
                time: git::now(),
                ..Commit::default()
            }));
        }
        // Each stash sits right above the commit it was made on, as a commit on no branch.
        let mut stashes_on: HashMap<&str, Vec<&Stash>> = HashMap::new();
        for stash in &self.stashes {
            stashes_on
                .entry(stash.base.as_str())
                .or_default()
                .push(stash);
        }
        self.commits
            .reserve(self.history.len() + self.stashes.len());
        // A merge put aside for an agent is something going on, as the changes are: at the top,
        // its lane running down to the commit it was put aside on.
        self.commits
            .extend(super::resolver::parked_rows(&self.resolver.parked));
        for commit in &visible {
            for stash in stashes_on.get(commit.hash.as_str()).into_iter().flatten() {
                self.commits.push(Arc::new(Commit {
                    hash: stash.hash.clone(),
                    parents: vec![stash.base.clone()],
                    author: stash.author.clone(),
                    email: stash.email.clone(),
                    time: stash.time,
                    date: stash.date.clone(),
                    summary: stash.message.clone(),
                    ..Commit::default()
                }));
            }
            // Shared with `history`, not copied: tens of thousands of commits may be loaded.
            self.commits.push(Arc::clone(commit));
        }
        if let Some(pinned) = &self.history_filter.pinned
            && selected_hash.as_ref() == Some(&pinned.hash)
            && !self.commits.iter().any(|c| c.hash == pinned.hash)
        {
            let at = usize::from(self.commits.first().is_some_and(|c| c.hash == WIP_HASH));
            self.commits.insert(at, Arc::clone(pinned));
        }
        self.insert_pr_graph_head();
        let pinned_layout = self
            .history_filter
            .pinned_layout
            .as_ref()
            .map(|p| super::history_filter::pinned_projection(p, &self.history, &visible));
        let layout_by_hash: HashMap<&str, &Arc<Commit>> =
            graph_history.iter().map(|c| (c.hash.as_str(), c)).collect();
        let layout_rows: Vec<Arc<Commit>> = self
            .commits
            .iter()
            .map(|c| {
                if let Some(pinned) = pinned_layout.as_ref().filter(|p| p.hash == c.hash) {
                    return Arc::clone(pinned);
                }
                layout_by_hash
                    .get(c.hash.as_str())
                    .map_or_else(|| Arc::clone(c), |c| Arc::clone(c))
            })
            .collect();
        let graph = graph::layout_with(&layout_rows, self.log_order.first_parent);
        let index = self
            .commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let old_graph = std::mem::replace(&mut self.graph, graph);
        let old_index = std::mem::replace(&mut self.index, index);
        self.rebuild_graph_labels();
        self.queue_branch_bands();
        self.queue_graph_topology();
        self.cancel_minimap();
        // Freeing tens of thousands of commits (those a reload replaced), rows and index entries
        // takes a while: not on the UI thread.
        if old_rows.len() > 5000 {
            crate::connections::spawn(move || drop((old_rows, old_graph, old_index)));
        }
    }

    /// Keeps the selection; otherwise starts on the changes, or HEAD.
    fn restore_selection(&mut self, previous: Option<String>, ctx: &egui::Context) {
        if self
            .compare
            .as_ref()
            .is_some_and(|c| self.pr_selection_request == Some(c.request))
        {
            // A PR head not fetched yet has no row. Keep its comparison while a background
            // refresh chooses a temporary graph cursor; selecting HEAD would close it.
            self.selected = previous
                .as_ref()
                .and_then(|hash| self.index.get(hash).copied())
                .or_else(|| self.index.get(&self.refs.head).copied());
            return;
        }
        // A commit of the reflog view that the graph didn't have: shown there once it has it
        // (made a branch of), kept in the detail pane until then.
        // An operation that left conflicts goes to the changes row first.
        if let Some(hash) = self.reflog_commit_off_graph().filter(|_| !self.focus_wip) {
            if let Some(row) = self.index.get(&hash).copied() {
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            return;
        }
        let mut keep = previous.as_ref().and_then(|h| self.index.get(h).copied());
        if std::mem::take(&mut self.focus_wip) && self.index.contains_key(WIP_HASH) {
            keep = None;
            self.stash_view = None;
        }
        if self.stash_view.is_some() {
            self.selected = self
                .stash_view
                .and_then(|i| self.index.get(&self.stashes.get(i)?.hash).copied());
            return;
        }
        let target = keep
            .or_else(|| self.index.get(WIP_HASH).copied())
            .or_else(|| self.index.get(&self.refs.head).copied());
        match target {
            Some(row) if keep == Some(row) => self.selected = Some(row),
            Some(row) => {
                self.selected = None;
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            None => {
                self.selected = None;
                self.detail = None;
                self.detail_hash = None;
                self.drop_extras();
            }
        }
    }

    /// A merge or cherry-pick leaves a message to commit with.
    fn take_prepared_message(&mut self) {
        // Over an empty box, or one that holds only what the template and hook put there.
        if let Some(msg) = self.work.prepared_message.clone() {
            self.take_message(&msg);
        }
    }

    /// The diff of a file that's no longer listed (staged, committed…) is stale.
    fn refresh_work_diff(&mut self, ctx: &egui::Context) {
        let Some(DiffView {
            key: DiffKey::Work(file),
            ..
        }) = &self.diff
        else {
            return;
        };
        let still = self
            .work
            .staged
            .iter()
            .chain(&self.work.unstaged)
            .chain(&self.work.conflicted)
            .any(|f| f == file);
        let key = DiffKey::Work(file.clone());
        // The conflict editor keeps what was chosen (or Mergiraf's answer) while the file
        // is as it was opened.
        // Elsewhere, kept, and looked at in the background (see `conflict_changed`).
        let remote = crate::remote::is_remote(&self.path);
        let editing = self
            .conflict
            .as_ref()
            .is_some_and(|ed| ed.path == file.path && (remote || !ed.changed_on_disk(&self.path)));
        if remote
            && editing
            && let Some(ed) = &self.conflict
        {
            let (repo, path, opened) = (self.path.clone(), ed.path.clone(), ed.opened().to_owned());
            let generation = ed.generation;
            spawn(&self.tx, ctx, move || {
                let changed = super::conflict::changed_on_disk(&repo, &path, &opened);
                // Nothing to say when it didn't, nor when it couldn't be read: a dropped
                // connection is no reason to drop the choices made.
                Msg::ConflictChanged(generation, (changed == Some(true)).then_some(path))
            });
        }
        if still && (file.area != git::Area::Conflicted || editing) {
            // Shown as it was until the new diff is read (no flicker after staging lines).
            self.refresh_diff(ctx);
        } else {
            self.diff = None;
            self.conflict = None;
            if still {
                self.open_diff(key, ctx);
            }
        }
    }

    /// Once per tab, on a forge's repository: tells the avatar loader where its authors' account
    /// pictures are.
    pub(super) fn hint_avatars(&mut self, ctx: &egui::Context) {
        // Offline or metered: asked on the next read once that is over.
        if self.avatars_hinted
            || !Settings::get(ctx).avatars
            || !crate::network::background_may_run()
        {
            return;
        }
        // A local mirror has no host to ask.
        let Some(hosting) = self.hosting.clone().filter(|h| !h.host.is_empty()) else {
            return;
        };
        self.avatars_hinted = true;
        let repo = self.path.clone();
        let mut emails: Vec<String> = self.history.iter().map(|c| c.email.clone()).collect();
        emails.sort();
        emails.dedup();
        self.forge_then(
            ctx,
            move || hosting.forge.provider().avatars(&repo, &hosting, &emails),
            |_, hints, ctx| {
                for (email, hint) in hints {
                    crate::avatars::hint(&email, hint);
                }
                ctx.request_repaint();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    /// The pull requests read before a merge answer after those read since: the merged one
    /// doesn't come back.
    #[test]
    fn an_older_list_of_pull_requests_answering_last_is_dropped() {
        use std::time::{Duration, Instant};

        use super::super::{Msg, RepoTab};
        use crate::forge::PullRequest;

        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/prs-order-test".into(), &ctx);
        let merged = PullRequest {
            number: 7,
            title: "merged since".to_owned(),
            branch: "feat".to_owned(),
            base: "main".to_owned(),
            from_fork: false,
            sha: String::new(),
            author: String::new(),
            url: String::new(),
            draft: false,
        };
        let before = Instant::now();
        let since = before + Duration::from_millis(1);
        tab.prs_loaded = Some(since);
        for msg in [
            Msg::Prs(since, Ok(Vec::new())),
            Msg::Prs(before, Ok(vec![merged])),
        ] {
            let _ = tab.tx.send(msg);
        }
        tab.poll(&ctx);
        assert!(tab.prs.is_empty());
    }
}

#[cfg(test)]
mod connection_identity_tests {
    use super::*;
    #[test]
    fn connection_edits_clear_tab_and_parked_credentials_before_another_action() {
        let a = crate::connections::Context {
            error: Some("profile a".into()),
            ..Default::default()
        };
        let b = crate::connections::Context {
            error: Some("profile b".into()),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut tab =
            a.run(|| RepoTab::new_deferred("/nonexistent/context-edit-main".into(), &ctx));
        let mut parked =
            a.run(|| RepoTab::new_deferred("/nonexistent/context-edit-worktree".into(), &ctx));
        let creds = git::Credentials {
            scope: None,
            username: "alice".into(),
            password: "never-send-as-bob".into(),
        };
        tab.set_credentials(creds.clone());
        parked.set_credentials(creds);
        tab.parked.push(Box::new(parked));
        b.run(|| tab.notice_connections(&ctx));
        assert!(tab.credentials.is_none());
        assert!(tab.parked[0].credentials.is_none());
        assert_eq!(tab.connection_context, b);
    }
}
