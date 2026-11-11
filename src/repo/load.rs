//! Reading the repository in the background and rebuilding the graph rows from what was read.

#[path = "core_seams/load_requests.rs"]
mod load_requests;

#[path = "core_seams/history_projection.rs"]
mod history_projection;

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
                // The mutation may have succeeded before the subsequent reload failed.
                // Reject pre-mutation file reads even when no fresh repository data arrives.
                self.invalidate_mutable_file_view(ctx);
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
        // What was shown from a snapshot counts as moved: nothing was read against it.
        let from_snapshot = std::mem::take(&mut self.snapshot_shown);
        let refs_moved = loaded.refs != self.refs || from_snapshot;
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
        // The other worktrees' last counts stay until they are counted again (just below).
        for wt in self.worktrees.iter_mut().filter(|w| w.needs_count()) {
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
        self.keep_snapshot();
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
}

#[cfg(test)]
#[path = "direct_tests/load_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "direct_tests/load_connection_identity_tests.rs"]
mod connection_identity_tests;

impl RepoTab {
    /// Apply a fresh status snapshot and refresh only the affected presentation.
    pub(in crate::repo) fn status_arrived(
        &mut self,
        read: u64,
        result: Result<WorkStatus, String>,
        ctx: &egui::Context,
    ) {
        match result {
            Ok(work) => {
                crate::timing::log(format_args!(
                    "status#{read} came in: {} changed files",
                    work.changed_files()
                ));
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
                self.status_changed_file_view(ctx);
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
            Err(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
        }
    }
}
