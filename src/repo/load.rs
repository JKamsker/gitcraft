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
    pub(super) work: WorkStatus,
    pub(super) worktrees: Vec<Worktree>,
    pub(super) stashes: Vec<Stash>,
    pub(super) remotes: Vec<Remote>,
    /// The forge `origin` is on, if gitcraft knows it.
    pub(super) hosting: Option<Hosting>,
    pub(super) submodules: Vec<Submodule>,
    /// It keeps files in Git LFS, and git-lfs is installed.
    pub(super) lfs: bool,
    pub(super) identity: Identity,
    /// How many commits were asked for (more exist when that many came back).
    pub(super) requested: usize,
    /// What `history` was logged from.
    pub(super) log_from: Option<Vec<String>>,
    /// Which read this is (see `RepoTab::reads`).
    pub(super) read: u64,
}

/// Whether two hostings are the same repository on the same forge, however it's reached (HTTPS
/// or SSH): the first load, with nothing before it, moves nothing anywhere.
pub(super) fn same_repository(before: Option<&Hosting>, now: Option<&Hosting>) -> bool {
    match (before, now) {
        (None, _) => true,
        (Some(a), Some(b)) => {
            a.forge == b.forge && a.host == b.host && a.path.eq_ignore_ascii_case(&b.path)
        }
        (Some(_), None) => false,
    }
}

impl RepoTab {
    /// Re-reads refs, history, working tree, worktrees, stashes, remotes and submodules in the
    /// background.
    pub fn reload(&mut self, ctx: &egui::Context) {
        // Not read yet, or suspended: woken as when shown, which watches and reads once.
        if self.sleeping() {
            self.start_deferred(ctx);
            return;
        }
        // Changes seen on disk so far are read by this reload (or the one queued after the
        // current one): reloading for them again would read the same.
        self.fs_pending = None;
        if self.loading {
            // What changed may have been read before it changed: read again once this is in.
            self.reload_again = true;
            return;
        }
        self.loading = true;
        let read = self.next_read();
        let path = self.path.clone();
        // Keep the pages already loaded.
        let count = self.history.len().clamp(git::LOG_PAGE, MAX_HISTORY);
        let mut visibility = Visibility::load(ctx, self.repo_key());
        // Answered all the same if it panics: the tab reloads only once it is.
        let failed = |e| Msg::Loaded(Err(e));
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            Msg::Loaded((|| {
                let refs = git::refs(&path)?;
                visibility.prune(&refs);
                let log_from = visibility.revisions(&refs);
                // Independent reads, side by side: a reload waits for the slowest of them
                // rather than for all of them in turn.
                let path = path.as_path();
                let (history, work, worktrees, stashes, remotes, submodules, lfs, identity) =
                    std::thread::scope(|scope| {
                        let history = scope.spawn(|| git::log(path, log_from.as_deref(), 0, count));
                        let work_trees = scope.spawn(|| {
                            let work = git::status(path)?;
                            let changes = Some(work.changed_files());
                            // Very old gits have no `worktree list`; the section just stays
                            // empty.
                            // Elsewhere, the others are counted after the tab shows.
                            let worktrees = if crate::remote::is_remote(path) {
                                git::worktree_list(path, changes)
                            } else {
                                git::worktrees(path, changes)
                            };
                            let worktrees = worktrees.unwrap_or_default();
                            Ok::<_, String>((work, worktrees))
                        });
                        let stashes = scope.spawn(|| git::stashes(path).unwrap_or_default());
                        let remotes = scope.spawn(|| git::remotes(path).unwrap_or_default());
                        let submodules = scope.spawn(|| git::submodules(path).unwrap_or_default());
                        let lfs = scope.spawn(|| git::uses_lfs(path) && git::lfs_installed(path));
                        let identity = git::identity(path);
                        let join = |e| format!("Reading the repository failed: {e:?}");
                        let history: Vec<Arc<Commit>> = history
                            .join()
                            .map_err(join)??
                            .into_iter()
                            .map(Arc::new)
                            .collect();
                        let (work, worktrees) = work_trees.join().map_err(join)??;
                        Ok::<_, String>((
                            history,
                            work,
                            worktrees,
                            stashes.join().map_err(join)?,
                            remotes.join().map_err(join)?,
                            submodules.join().map_err(join)?,
                            lfs.join().map_err(join)?,
                            identity,
                        ))
                    })?;
                let hosting = forge::detect(path, &remotes);
                Ok(Box::new(Loaded {
                    refs,
                    history,
                    work,
                    worktrees,
                    stashes,
                    remotes,
                    hosting,
                    submodules,
                    lfs,
                    identity,
                    requested: count,
                    log_from,
                    read,
                }))
            })())
        });
    }

    pub(super) fn reload_status(&mut self, ctx: &egui::Context) {
        let read = self.next_read();
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
        if self.loading_more || !self.has_more || self.history.len() >= MAX_HISTORY {
            return;
        }
        self.loading_more = true;
        let path = self.path.clone();
        let skip = self.history.len();
        let generation = self.history_gen;
        let from = self.log_from.clone();
        let stream = Arc::clone(&self.log_stream);
        // Answered all the same if it panics: no more is read until it is.
        let failed = move |e| Msg::More(generation, Err(e));
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            let page = (|| {
                let mut slot = stream
                    .lock()
                    .map_err(|_| "Reading older history failed".to_owned())?;
                // The first page of this history (since it was last reloaded) starts the
                // `git log` the next ones are read from.
                if slot
                    .as_ref()
                    .is_none_or(|(made_for, _)| *made_for != generation)
                {
                    *slot = None;
                    *slot = Some((
                        generation,
                        git::LogStream::start(&path, from.as_deref(), skip)?,
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
            Msg::More(generation, page)
        });
    }

    pub(super) fn load_prs(&mut self, ctx: &egui::Context) {
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let asked = Instant::now();
        self.prs_loaded = Some(asked);
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            Msg::Prs(asked, hosting.forge.provider().list(&path, &hosting))
        });
    }

    /// Takes in what `reload` read, then reads again if a reload was asked for meanwhile.
    pub(super) fn loaded(&mut self, result: Result<Box<Loaded>, String>, ctx: &egui::Context) {
        self.loading = false;
        let resumed = std::mem::take(&mut self.resuming);
        match result {
            Ok(loaded) => self.take_loaded(*loaded, ctx),
            Err(e) => self.load_error = Some(e),
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
        // A page asked for before this was read past the old history, which
        // this one replaces: it would leave a gap.
        self.history_gen += 1;
        // The `git log` older pages came from read the history replaced: let it go (once a
        // page being read from it is in).
        self.log_stream = Default::default();
        self.loading_more = false;
        self.refs = loaded.refs;
        self.has_more = loaded.history.len() >= loaded.requested;
        self.history = loaded.history;
        // A status read since this reload started is newer.
        if Self::newer(loaded.read, &mut self.work_read) {
            self.work = loaded.work;
        }
        // The last counts of worktrees this read left uncounted (on another machine) stay
        // until they are counted again.
        let counted: Vec<(PathBuf, Option<usize>)> = self
            .worktrees
            .iter()
            .map(|w| (w.path.clone(), w.changes))
            .collect();
        self.worktrees = loaded.worktrees;
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
        self.take_visibility(loaded.log_from, ctx);
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
        // Another project (origin pointed elsewhere): its pull requests are other ones, under
        // the same numbers, and a list asked of the old one is not to be taken in.
        let remotes_changed = loaded.remotes != self.remotes;
        self.remotes = loaded.remotes;
        if loaded.hosting != self.hosting || remotes_changed {
            let moved = !same_repository(self.hosting.as_ref(), loaded.hosting.as_ref());
            self.hosting = loaded.hosting;
            self.prs.clear();
            self.pr_error = None;
            self.prs_loaded = None;
            if moved {
                // The page and what was written for it are another repository's now: sent, they
                // would go to this one, under that number.
                self.close_pr();
                self.ci_view = None;
                self.pr_drafts.clear();
                self.avatars_hinted = false;
            }
        }
        if self
            .prs_loaded
            .is_none_or(|t| t.elapsed() > Duration::from_secs(120))
        {
            self.load_prs(ctx);
        }
        self.refresh_ci(ctx);
        self.rebuild(ctx);
        self.hint_avatars(ctx);
        self.reload_compare(ctx);
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
            Err(e) => self.status = Some((e, true)),
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
    fn rebuild_rows(&mut self) {
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
        for commit in &self.history {
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
        let graph = graph::layout(&self.commits);
        let index = self
            .commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let old_graph = std::mem::replace(&mut self.graph, graph);
        let old_index = std::mem::replace(&mut self.index, index);
        // Freeing tens of thousands of commits (those a reload replaced), rows and index entries
        // takes a while: not on the UI thread.
        if old_rows.len() > 5000 {
            std::thread::spawn(move || drop((old_rows, old_graph, old_index)));
        }
    }

    /// Keeps the selection; otherwise starts on the changes, or HEAD.
    fn restore_selection(&mut self, previous: Option<String>, ctx: &egui::Context) {
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
            }
        }
    }

    /// A merge or cherry-pick leaves a message to commit with.
    fn take_prepared_message(&mut self) {
        if let Some(msg) = &self.work.prepared_message
            && self.commit_summary.is_empty()
            && self.commit_body.is_empty()
        {
            let (summary, body) = msg.split_once('\n').unwrap_or((msg.as_str(), ""));
            self.commit_summary = summary.trim().to_owned();
            self.commit_body = body.trim().to_owned();
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
        if self.avatars_hinted || !Settings::get(ctx).avatars {
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
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (email, hint) in hosting.forge.provider().avatars(&repo, &hosting, &emails) {
                crate::avatars::hint(&email, hint);
            }
            ctx.request_repaint();
        });
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
