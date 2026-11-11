//! Selecting commits and stashes, opening diffs, comparisons and file views, and the commit
//! search.

#[path = "core_seams/diff_navigation.rs"]
mod diff_navigation;

#[path = "core_seams/file_navigation.rs"]
mod file_navigation;

use super::{
    Action, Compare, CompareData, DiffKey, DiffView, MAX_HISTORY, Msg, RepoTab, WIP_HASH, conflict,
    diff, file_text, file_view, spawn,
};
use crate::git;
use crate::settings::Settings;
use crate::theme::Tokens;

/// A delayed graph selection retains whether it came from browsing or view restoration.
#[derive(Clone)]
pub(super) enum PendingGoto {
    History(String),
    Restore(String),
}

impl PendingGoto {
    pub(super) fn hash(&self) -> &str {
        match self {
            Self::History(hash) | Self::Restore(hash) => hash,
        }
    }

    pub(super) fn restoring(&self) -> bool {
        matches!(self, Self::Restore(_))
    }
}

impl RepoTab {
    pub(super) fn retire_navigation_restore(&mut self, action: &Action) {
        if self
            .pending_goto
            .as_ref()
            .is_some_and(PendingGoto::restoring)
            && matches!(
                action,
                Action::Select(_)
                    | Action::SelectHash(_)
                    | Action::SelectStash(_)
                    | Action::ActivityCommit(_)
                    | Action::OpenCommitChanges(_)
                    | Action::CloseDiff
                    | Action::FileView { .. }
                    | Action::OpenPr(_)
                    | Action::OpenCi { .. }
                    | Action::Navigate(_)
            )
        {
            self.pending_goto = None;
        }
    }

    pub(super) fn is_wip(&self, row: usize) -> bool {
        self.commits.get(row).is_some_and(|c| c.hash == WIP_HASH)
    }

    /// The stash a graph row stands for, if it's one.
    pub(super) fn stash_at(&self, row: usize) -> Option<usize> {
        let hash = &self.commits.get(row)?.hash;
        self.stashes.iter().position(|s| &s.hash == hash)
    }

    pub(super) fn selected_hash(&self) -> Option<String> {
        self.selected
            .and_then(|r| self.commits.get(r))
            .map(|c| c.hash.clone())
    }

    pub(super) fn select(&mut self, row: usize, ctx: &egui::Context) {
        let Some(commit) = self.commits.get(row) else {
            return;
        };
        let stash = self.stash_at(row);
        if self.selected == Some(row) && self.stash_view == stash {
            return;
        }
        let hash = commit.hash.clone();
        // Below the graph, the diff stays and follows what is opened next.
        let keep_diff = self.diff_beside_graph(ctx);
        self.selected = Some(row);
        self.stash_view = stash;
        self.file_view = None;
        self.compare = None;
        if !keep_diff
            && (matches!(
                self.diff,
                Some(DiffView {
                    key: DiffKey::Commit { .. } | DiffKey::Range { .. } | DiffKey::Remerge { .. },
                    ..
                })
            ) || hash != WIP_HASH)
        {
            self.diff = None;
            self.conflict = None;
        }
        if hash == WIP_HASH {
            self.detail = None;
            self.detail_hash = None;
            self.drop_extras();
            return;
        }
        // A merge put aside: its agent's card, and its files (as the commit they are kept as).
        if let Some(snapshot) = self.parked_at(row).map(|p| p.snapshot.clone()) {
            self.drop_extras();
            self.resolver_show();
            self.fetch_detail(snapshot, ctx);
            return;
        }
        self.load_detail(hash, ctx);
    }

    pub(super) fn load_detail(&mut self, hash: String, ctx: &egui::Context) {
        self.commit_changes.cancel_pending();
        // A row of no commit (the changes row, a merge put aside): nothing for git to read.
        if hash.starts_with('\u{0}') {
            return;
        }
        self.detail = None;
        self.reword = None;
        self.detail_error = None;
        self.detail_hash = Some(hash.clone());
        self.load_extras(&hash, ctx);
        self.fetch_detail(hash, ctx);
    }

    /// Reads the commit `hash`'s detail (its message, files…) in the background, without what
    /// the commit panel adds (its builds, branches, notes): for a merge put aside's files.
    pub(super) fn fetch_detail(&mut self, hash: String, ctx: &egui::Context) {
        self.detail = None;
        self.detail_error = None;
        self.detail_hash = Some(hash.clone());
        // What was still being read for the commit selected before is not waited for.
        self.detail_cancel.cancel();
        self.detail_cancel = self.tx.cancel.child();
        if let Some(detail) = self.detail_cache.get(&hash, &self.refs.head) {
            self.show_detail(detail, ctx);
            return;
        }
        self.detail_asked = Some(std::time::Instant::now());
        let path = self.path.clone();
        let failed_hash = hash.clone();
        let (cancel, failed_cancel) = (self.detail_cancel.clone(), self.detail_cancel.clone());
        // A read the selection moved on from only fills the cache: should the same commit be
        // selected again meanwhile, its "cancelled" is not that one's error.
        super::messages::spawn_selection(
            &self.tx,
            ctx,
            self.detail_cancel.clone(),
            false,
            move |error| {
                if failed_cancel.is_cancelled() {
                    Msg::DetailPrefetched(Err(error))
                } else {
                    Msg::Detail(failed_hash, Err(error))
                }
            },
            move || {
                let detail = git::commit_detail(&path, &hash);
                if cancel.is_cancelled() {
                    Msg::DetailPrefetched(detail)
                } else {
                    Msg::Detail(hash, detail)
                }
            },
        );
    }

    /// The selected commit's detail, read or from the cache.
    pub(super) fn show_detail(&mut self, detail: git::CommitDetail, ctx: &egui::Context) {
        let hash = detail.hash.clone();
        self.detail = Some(detail);
        self.fill_reword();
        self.finish_commit_changes(&hash, ctx);
    }

    /// Reads ahead the files of the commits on screen (`rows`, and as many again around them),
    /// one batch at a time behind all other work: a click on any of them shows its files at once.
    pub(super) fn prefetch_files(&mut self, rows: std::ops::Range<usize>, ctx: &egui::Context) {
        /// Commits a batch reads at most.
        const BATCH: usize = 64;
        // Not while the repository is read: that read is what the tab waits on.
        if self.files_cache.reading || self.loading || self.commits.is_empty() {
            return;
        }
        let around = rows.len().max(20);
        let start = rows.start.saturating_sub(around);
        let end = (rows.end + around).min(self.commits.len());
        // Those on screen first, then those just off it.
        let order = rows
            .clone()
            .chain(rows.end..end)
            .chain((start..rows.start).rev());
        let hashes: Vec<String> = order
            .filter_map(|row| {
                let hash = &self.commits.get(row)?.hash;
                (hash != WIP_HASH
                    && !hash.starts_with('\u{0}')
                    && self.stash_at(row).is_none()
                    && self.parked_at(row).is_none()
                    && !self.files_cache.settled(hash))
                .then(|| hash.clone())
            })
            .take(BATCH)
            .collect();
        if hashes.is_empty() {
            return;
        }
        self.files_cache.reading = true;
        let path = self.path.clone();
        let (asked, failed_asked) = (hashes.clone(), hashes.clone());
        super::messages::spawn_selection(
            &self.tx,
            ctx,
            self.tx.cancel.clone(),
            true,
            move |error| Msg::FilesPrefetched(failed_asked, Err(error)),
            move || Msg::FilesPrefetched(asked, git::commit_files_batch(&path, &hashes)),
        );
    }

    pub(super) fn select_stash(&mut self, i: usize, ctx: &egui::Context) {
        let Some(stash) = self.stashes.get(i) else {
            return;
        };
        let hash = stash.hash.clone();
        self.stash_view = Some(i);
        self.selected = self.index.get(&hash).copied();
        self.scroll_to = self.selected;
        self.file_view = None;
        self.compare = None;
        self.diff = None;
        self.conflict = None;
        self.load_detail(hash, ctx);
    }

    pub(super) fn select_hash(&mut self, hash: &str, ctx: &egui::Context) {
        self.select_target(&PendingGoto::History(hash.to_owned()), ctx);
    }

    fn select_target(&mut self, target: &PendingGoto, ctx: &egui::Context) {
        let hash = target.hash();
        match self.index.get(hash).copied() {
            Some(row) => {
                if target.restoring() {
                    self.restore_commit_selection(row, ctx);
                } else {
                    self.select_history(row, ctx);
                }
                self.scroll_to = Some(row);
            }
            None => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-2900",
                        &[("hash", hash.get(..8).unwrap_or(hash).to_string().into())],
                    ),
                    true,
                )));
            }
        }
    }

    /// Selects `hash`, loading more history first if it's further down.
    pub(super) fn goto(&mut self, hash: &str, ctx: &egui::Context) {
        self.goto_target(PendingGoto::History(hash.to_owned()), ctx);
    }

    pub(super) fn goto_target(&mut self, target: PendingGoto, ctx: &egui::Context) {
        let hash = target.hash().to_owned();
        // Nothing read to look in yet: once it is (see `rebuild`).
        if self.sleeping() || self.resuming {
            self.pending_goto = Some(target);
            return;
        }
        if !self.index.contains_key(&hash) && self.history.iter().any(|c| c.hash == hash) {
            self.history_filter.revealed.insert(hash.clone());
            let previous = self.selected_hash();
            self.rebuild_rows();
            self.selected = previous.and_then(|h| self.index.get(&h).copied());
        }
        if self.index.contains_key(&hash) {
            self.pending_goto = None;
            self.select_target(&target, ctx);
            self.history_filter.revealed.remove(&hash);
        } else if self.has_more && self.history.len() < MAX_HISTORY {
            self.pending_goto = Some(target);
            self.load_more(ctx);
        } else {
            self.pending_goto = None;
            self.select_target(&target, ctx);
        }
    }

    /// Closes the diff of a file between two commits (opened from a comparison).
    pub(super) fn close_range_diff(&mut self) {
        if matches!(
            self.diff,
            Some(DiffView {
                key: DiffKey::Range { .. },
                ..
            })
        ) {
            self.diff = None;
        }
    }

    /// Starts comparing two points of history (replacing the comparison shown).
    pub(super) fn compare(&mut self, compare: Compare, ctx: &egui::Context) {
        self.tour = None;
        self.pr_selection_request = None;
        self.close_pr();
        self.close_range_diff();
        self.file_view = None;
        self.compare = Some(compare);
        self.reload_compare(ctx);
    }

    /// Reads the comparison's files again (the working tree or a branch moved).
    pub(super) fn reload_compare(&mut self, ctx: &egui::Context) {
        // Background file-system refreshes must not replace the request still resolving a
        // PR's exact endpoints with a comparison against stale tracking refs.
        if self
            .compare
            .as_ref()
            .is_some_and(|c| self.pr_selection_request == Some(c.request))
        {
            return;
        }
        self.compare_requests += 1;
        let request = self.compare_requests;
        let Some(c) = &mut self.compare else { return };
        c.request = request;
        let (path, base, target, since_fork) = (
            self.path.clone(),
            c.base.clone(),
            c.target.clone(),
            c.since_fork,
        );
        spawn(&self.tx, ctx, move || {
            let result = (|| {
                let base = git::resolve(&path, &base)?;
                let to = target.map(|t| git::resolve(&path, &t)).transpose()?;
                // The working tree goes with HEAD's history.
                let other = match &to {
                    Some(to) => to.clone(),
                    None => git::resolve(&path, "HEAD")?,
                };
                let from = if since_fork {
                    git::merge_base(&path, &base, &other)?
                } else {
                    base.clone()
                };
                Ok(CompareData {
                    files: git::diff_files(&path, &from, to.as_deref())?,
                    apart: git::count_apart(&path, &base, &other)?,
                    from,
                    to,
                })
            })();
            Msg::Compare(request, result)
        });
    }

    /// Swaps the comparison's two ends.
    pub(super) fn swap_compare(&mut self, ctx: &egui::Context) {
        // The working tree stays the newer end.
        if let Some(c) = self.compare.take_if(|c| c.target.is_some())
            && let Some(target) = c.target
        {
            self.compare(
                Compare {
                    base: target,
                    base_label: c.target_label,
                    target: Some(c.base),
                    target_label: c.base_label,
                    captions: [c.captions[1].clone(), c.captions[0].clone()],
                    data: None,
                    ..c
                },
                ctx,
            );
        }
    }
}

#[cfg(test)]
#[path = "direct_tests/navigate_tests.rs"]
mod tests;
