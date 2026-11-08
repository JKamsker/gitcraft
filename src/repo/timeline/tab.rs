//! The tab's side of the timeline: reading it from disk when the tab opens, an entry for each
//! operation once the read after it says what it did, one for each change found made
//! elsewhere, undo / redo / jumps, and keeping it on disk.

use std::collections::HashSet;

use super::store::{self, Saved};
use super::{Awaiting, Ended, Entry, Origin, RefState, observe, snapshot};
use crate::git;
use crate::repo::{OpKind, Refresh, RepoTab, Undo, UndoEntry};

impl RepoTab {
    /// A multi-step feature finished outside run_ops; the next read records its undo.
    pub(in crate::repo) fn push_undo(&mut self, entry: UndoEntry, ctx: &egui::Context) {
        let label = entry.label.clone();
        self.timeline_op_started(false, Refresh::All);
        self.timeline_op_ended(
            &label,
            &OpKind::plain(),
            Some(entry),
            true,
            Refresh::All,
            ctx,
        );
        self.reload(ctx);
    }

    /// Resuming a release fills in the same undo entry rather than making a second step.
    pub(in crate::repo) fn replace_recent_undo(
        &mut self,
        mut undo: UndoEntry,
        ctx: &egui::Context,
    ) {
        if let Some(entry) = self
            .timeline
            .entries
            .iter_mut()
            .rev()
            .find(|e| e.label == undo.label)
        {
            undo.id = entry.id;
            entry.undo = Some(undo);
            self.save_timeline(ctx);
        } else {
            self.push_undo(undo, ctx);
        }
    }

    pub(in crate::repo) fn update_recent_redo(
        &mut self,
        label: &str,
        redo: Vec<Vec<String>>,
        ctx: &egui::Context,
    ) {
        fn update(then: &mut Ended, label: &str, redo: &[Vec<String>]) {
            if let Ended::New {
                label: name,
                undo: Some(undo),
            } = then
                && name == label
            {
                undo.redo = redo.to_vec();
            }
        }
        if let Awaiting::Ended { then, .. } = &mut self.timeline.awaiting {
            update(then, label, &redo);
        }
        for then in &mut self.timeline.backlog {
            update(then, label, &redo);
        }
        if let Some(entry) = self
            .timeline
            .entries
            .iter_mut()
            .rev()
            .find(|e| e.label == label)
            && let Some(undo) = &mut entry.undo
        {
            undo.redo = redo;
        }
        self.save_timeline(ctx);
    }

    pub(in crate::repo) fn remember_undo(
        &mut self,
        label: &str,
        undo: UndoEntry,
        ctx: &egui::Context,
    ) {
        let state = self.ref_state();
        self.record(
            label.to_owned(),
            Origin::Gitcraft,
            state.clone(),
            state,
            Some(undo),
            ctx,
        );
    }

    /// Reads what was kept for this folder, in the background.
    pub(in crate::repo) fn load_timeline(&mut self, ctx: &egui::Context) {
        let folder = self.path.clone();
        self.then(
            ctx,
            move || store::load(&folder),
            |tab, saved, ctx| tab.adopt_timeline(saved, ctx),
        );
    }

    fn adopt_timeline(&mut self, saved: Result<Saved, String>, ctx: &egui::Context) {
        match saved {
            Ok(saved) => {
                let had_own = !self.timeline.entries.is_empty();
                self.timeline.adopt(saved);
                if had_own {
                    self.save_timeline(ctx);
                }
                self.release_stale_refs(ctx);
            }
            Err(e) => {
                // Kept in memory only: the file stays as it is.
                self.timeline.loaded = true;
                self.timeline.read_only = true;
                self.timeline_error = Some(e);
            }
        }
        if let Some((read, state)) = self.timeline.early_read.take() {
            self.take_state(read, state, ctx);
        }
    }

    /// An operation starts: what is read until it ends is only part of it. One that only
    /// changes the working tree or the index (`Refresh::Status`: staging) moves no branch, and
    /// leaves the timeline alone.
    pub(in crate::repo) fn timeline_op_started(&mut self, quiet: bool, refresh: Refresh) {
        if quiet || refresh == Refresh::Status {
            return;
        }
        // The one before still waits for its read: it is taken in with this one's.
        if let Awaiting::Ended { then, .. } = std::mem::take(&mut self.timeline.awaiting) {
            self.timeline.backlog.push(then);
        }
        self.timeline.awaiting = Awaiting::Running;
    }

    /// An operation ended (`ok` or not); `undo` is how to undo it, when it can be. What it
    /// did is known from the next read of the repository.
    pub(in crate::repo) fn timeline_op_ended(
        &mut self,
        label: &str,
        kind: &OpKind,
        undo: Option<UndoEntry>,
        ok: bool,
        refresh: Refresh,
        ctx: &egui::Context,
    ) {
        let jump = self.timeline.jump.take_if(|(l, _)| l == label);
        let then = match (kind, jump) {
            (_, Some((_, to))) if ok => Ended::Moved { to, undo: None },
            (OpKind::Undo(entry), _) if ok => Ended::Moved {
                to: self.timeline.get(entry.id).and_then(|e| e.parent),
                undo: Some(entry.clone()),
            },
            (OpKind::Redo(entry), _) if ok => Ended::Moved {
                to: Some(entry.id),
                undo: Some(entry.clone()),
            },
            // Stopped partway, it may have moved something all the same.
            _ if !ok => Ended::New {
                label: format!("{label} (stopped partway)"),
                undo: None,
            },
            _ => Ended::New {
                label: label.to_owned(),
                undo,
            },
        };
        if refresh == Refresh::Status {
            // No read of the branches follows, and it moved none: only one that can be undone
            // is an entry, at once.
            if let (
                Ended::New {
                    label,
                    undo: Some(undo),
                },
                Some(state),
            ) = (then, self.timeline.seen.clone())
            {
                self.record(
                    label,
                    Origin::Gitcraft,
                    state.clone(),
                    state,
                    Some(undo),
                    ctx,
                );
            }
            return;
        }
        self.timeline.awaiting = Awaiting::Ended {
            after_read: self.reads,
            then,
        };
    }

    /// The branches as the read numbered `read` found them.
    pub(in crate::repo) fn timeline_read(&mut self, read: u64, ctx: &egui::Context) {
        let state = self.ref_state();
        if !self.timeline.loaded {
            self.timeline.early_read = Some((read, state));
            return;
        }
        self.take_state(read, state, ctx);
    }

    fn take_state(&mut self, read: u64, state: RefState, ctx: &egui::Context) {
        match std::mem::take(&mut self.timeline.awaiting) {
            // Read while it ran (or before it ended): only part of it.
            awaiting @ Awaiting::Running => self.timeline.awaiting = awaiting,
            awaiting @ Awaiting::Ended { after_read, .. } if read <= after_read => {
                self.timeline.awaiting = awaiting;
            }
            Awaiting::Ended { then, .. } => {
                // Those that ended before it, whose own reads were missed: each is an entry,
                // the first with what they all moved.
                for earlier in std::mem::take(&mut self.timeline.backlog) {
                    self.resolve(earlier, state.clone(), true, ctx);
                }
                self.resolve(then, state, false, ctx);
            }
            Awaiting::Nothing => self.observe(state, ctx),
        }
    }

    /// What an operation that ended did, now that `state` says where the branches are; with
    /// `always`, it is an entry even if nothing it did shows any more.
    fn resolve(&mut self, then: Ended, state: RefState, always: bool, ctx: &egui::Context) {
        let before = self.timeline.seen.clone().unwrap_or_else(|| state.clone());
        let before = self.without_elsewhere(&before, &state);
        let seen_changed = self.timeline.seen.as_ref() != Some(&state);
        self.timeline.seen = Some(state.clone());
        match then {
            Ended::New { label, undo } => {
                if always || undo.is_some() || before != state {
                    self.record(label, Origin::Gitcraft, before, state, undo, ctx);
                } else if seen_changed {
                    self.save_timeline(ctx);
                }
            }
            Ended::Moved { to, undo } => {
                self.timeline.move_to(to, undo);
                self.save_timeline(ctx);
            }
        }
    }

    /// A read with no operation of gitcraft's behind it: what differs from the last one was
    /// done elsewhere.
    fn observe(&mut self, state: RefState, ctx: &egui::Context) {
        let Some(before) = self.timeline.seen.replace(state.clone()) else {
            self.save_timeline(ctx);
            return;
        };
        let before = self.without_elsewhere(&before, &state);
        if before != state {
            let label = observe::fallback_label(&super::changes(&before, &state));
            self.record(label, Origin::Outside, before, state, None, ctx);
        }
    }

    /// Adds an entry, then finds out its files (and what it was, for a change made elsewhere)
    /// and keeps its commits from gc, in the background.
    fn record(
        &mut self,
        label: String,
        origin: Origin,
        before: RefState,
        after: RefState,
        undo: Option<UndoEntry>,
        ctx: &egui::Context,
    ) {
        let (before, after) = super::sparse(&before, &after);
        let id = self.timeline.push(Entry {
            id: 0,
            parent: None,
            time: git::now(),
            label,
            origin,
            before,
            after,
            files: Vec::new(),
            files_total: 0,
            undo,
            redo_child: None,
            receipt: None,
        });
        self.save_timeline(ctx);
        self.release_dropped(ctx);
        let Some(entry) = self.timeline.get(id).cloned() else {
            return;
        };
        let (path, key) = (self.path.clone(), store::key(&self.path));
        self.then_or(
            ctx,
            false,
            |error| (observe::Found::default(), Err(error)),
            move || {
                let found = observe::find(&path, &entry);
                let kept = snapshot::keep(
                    &path,
                    &key,
                    entry.id,
                    &entry.label,
                    &snapshot::commits_of(&entry),
                );
                (found, kept)
            },
            move |tab, (found, kept), ctx| {
                let mut receipt_error = None;
                if let Some(entry) = tab.timeline.entries.iter_mut().find(|e| e.id == id) {
                    entry.files = found.files;
                    entry.files_total = found.files_total;
                    if let Some(label) = found.label {
                        entry.label = label;
                    }
                    match found.receipt {
                        Some(Ok(receipt)) => entry.receipt = Some(receipt),
                        Some(Err(e)) => receipt_error = Some(e),
                        None => {}
                    }
                }
                if let Some(e) = receipt_error {
                    tab.timeline_error = Some(crate::i18n::message(
                        "repo-workflows-timeline-record-56546037",
                        &[("e", (e).clone().into())],
                    ));
                }
                tab.show_receipt(id);
                let keep_ok = kept.is_ok();
                if let Err(e) = kept {
                    tab.timeline_error = Some(crate::i18n::message(
                        "repo-workflows-timeline-record-ef197d90",
                        &[
                            (
                                "tab-timeline-get-id-map-or-e-e-l-1",
                                (tab.timeline.get(id).map_or("", |e| e.label.as_str()))
                                    .clone()
                                    .into(),
                            ),
                            ("e", (e).clone().into()),
                        ],
                    ));
                }
                tab.save_timeline(ctx);
                tab.dco_timeline_kept(id, keep_ok, ctx);
            },
        );
    }

    /// Says what a rewrite gitcraft just did, in the status line, while that still says it's done.
    fn show_receipt(&mut self, id: u64) {
        let Some(entry) = self.timeline.get(id) else {
            return;
        };
        let (Some(receipt), Origin::Gitcraft) = (&entry.receipt, entry.origin) else {
            return;
        };
        let done = self
            .status
            .as_deref()
            .and_then(|(s, failed)| (!failed).then_some(s.as_str()))
            .and_then(|s| s.strip_suffix(": done"))
            .map(str::to_owned);
        if let Some(label) = done {
            self.status = Some(crate::repo::Notice::plain((
                format!(
                    "{label}: done. {} (receipt in the timeline, Ctrl+Shift+Z)",
                    receipt.sentence()
                ),
                false,
            )));
        }
    }

    /// Writes the timeline to disk in the background (not over a file that couldn't be read).
    pub(in crate::repo) fn save_timeline(&mut self, ctx: &egui::Context) {
        if !self.timeline.loaded || self.timeline.read_only {
            return;
        }
        let saved = Saved::of(&self.timeline, &self.path);
        let generation = store::next_generation();
        let folder = self.path.clone();
        self.then(
            ctx,
            move || store::save(&folder, &saved, generation),
            |tab, result, _| {
                if let Err(e) = result {
                    tab.timeline_error = Some(e);
                }
            },
        );
    }

    /// Lets go, in the background, of the commits of the entries forgotten.
    pub(in crate::repo) fn release_dropped(&mut self, ctx: &egui::Context) {
        let ids = std::mem::take(&mut self.timeline.dropped);
        if ids.is_empty() {
            return;
        }
        let (path, key) = (self.path.clone(), store::key(&self.path));
        self.then(
            ctx,
            move || snapshot::release(&path, &key, &ids),
            |tab, result, _| {
                if let Err(e) = result {
                    tab.timeline_error = Some(crate::i18n::message(
                        "repo-workflows-timeline-release-dropped-005d7b4f",
                        &[("e", (e).clone().into())],
                    ));
                }
            },
        );
    }

    /// Lets go of refs kept for entries no longer in the timeline.
    fn release_stale_refs(&mut self, ctx: &egui::Context) {
        let ids: HashSet<u64> = self.timeline.entries.iter().map(|e| e.id).collect();
        let (path, key) = (self.path.clone(), store::key(&self.path));
        self.then(
            ctx,
            move || snapshot::release_others(&path, &key, &ids),
            |_, _, _| {},
        );
    }

    /// Undoes the entry the repository is in: with its commands when gitcraft made it, else by
    /// putting the branches back as they were before it.
    pub(in crate::repo) fn undo(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() || self.timeline_unsettled("Undo", crate::repo::Action::Undo) {
            return;
        }
        let Some(entry) = self.timeline.current_entry().cloned() else {
            self.status = Some(crate::repo::Notice::plain((
                "Nothing to undo".to_owned(),
                false,
            )));
            return;
        };
        let label = format!("Undo {}", entry.label);
        match entry.undo {
            Some(undo) => {
                let steps = undo.undo.clone();
                self.run_ops(&label, steps, Refresh::All, OpKind::Undo(undo), ctx);
            }
            None => {
                let target = self.timeline.state_at(&self.ref_state(), entry.parent);
                self.restore(&label, &target, entry.parent, ctx);
            }
        }
    }

    /// Redoes the entry below the current one that was last undone (or made).
    pub(in crate::repo) fn redo(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() || self.timeline_unsettled("Redo", crate::repo::Action::Redo) {
            return;
        }
        let Some(entry) = self.timeline.redo_target().cloned() else {
            self.status = Some(crate::repo::Notice::plain((
                "Nothing to redo".to_owned(),
                false,
            )));
            return;
        };
        let label = format!("Redo {}", entry.label);
        match entry.undo {
            Some(undo) => {
                let steps = undo.redo.clone();
                self.run_ops(&label, steps, Refresh::All, OpKind::Redo(undo), ctx);
            }
            None => {
                let target = self.timeline.state_at(&self.ref_state(), Some(entry.id));
                self.restore(&label, &target, Some(entry.id), ctx);
            }
        }
    }

    /// Goes to entry `to`'s state (`None`: the one before every entry), wherever it is in the
    /// tree: the branches and HEAD are put back as it left them.
    pub(in crate::repo) fn jump(&mut self, to: Option<u64>, ctx: &egui::Context) {
        let again = crate::repo::Action::Timeline(super::view::TimelineAct::Jump(to));
        if self.busy_op.is_some() || self.timeline_unsettled("Going there", again) {
            return;
        }
        let Some((label, target)) = self.jump_target(to) else {
            return;
        };
        if let Some(skipped) = self
            .timeline
            .command_only_between(self.timeline.current, to)
        {
            self.status = Some(crate::repo::Notice::plain((
                format!(
                    "{label}: that would skip “{skipped}”, which moved no branch: step through it \
                     with {cmd}+Z / {cmd}+Y instead",
                    cmd = crate::manual::command()
                ),
                true,
            )));
            return;
        }
        self.restore(&label, &target, to, ctx);
    }

    /// Whether the timeline doesn't know yet where the last operation left the repository (its
    /// read hasn't come in): then `what` waits, and says so.
    fn timeline_unsettled(&mut self, what: &str, again: crate::repo::Action) -> bool {
        let unsettled = self.timeline.awaiting != Awaiting::Nothing || !self.timeline.loaded;
        if !unsettled {
            return false;
        }
        if self.loading || !self.timeline.loaded {
            // Asked again on the next frame, until the read under way is in.
            self.queued.push(again);
            self.status = Some(crate::repo::Notice::plain((
                format!("{what}: waiting for the repository to be read again…"),
                false,
            )));
        } else {
            // No read under way (the last one failed): it would wait for good.
            self.status = Some(crate::repo::Notice::plain((
                format!("{what}: the repository couldn't be read again; reload it (F5) first"),
                true,
            )));
        }
        true
    }

    /// HEAD and the local branches as the tab last read them.
    pub(in crate::repo) fn ref_state(&self) -> RefState {
        RefState::of(&self.refs)
    }

    /// `before` with the branches checked out in other worktrees as `after` has them: they move
    /// with the work done there, which is on those worktrees' timelines, not this one's.
    fn without_elsewhere(&self, before: &RefState, after: &RefState) -> RefState {
        let mut before = before.clone();
        for branch in self.other_worktree_branches() {
            match after.branches.get(&branch) {
                Some(commit) => before.branches.insert(branch, commit.clone()),
                None => before.branches.remove(&branch),
            };
        }
        before
    }

    /// What going to `to` is called, and the state it puts back.
    pub(in crate::repo) fn jump_target(&self, to: Option<u64>) -> Option<(String, RefState)> {
        let label = match to {
            Some(id) => format!("Go to “{}”", self.timeline.get(id)?.label),
            None => "Go to before the timeline".to_owned(),
        };
        let now = self.ref_state();
        Some((label, self.timeline.state_at(&now, to)))
    }

    /// Puts the branches and HEAD as `target` has them; once done, the timeline is at `to`.
    fn restore(&mut self, label: &str, target: &RefState, to: Option<u64>, ctx: &egui::Context) {
        let now = self.ref_state();
        match snapshot::restore_steps(&now, target, &self.other_worktree_branches()) {
            Ok(steps) => {
                self.timeline.jump = Some((label.to_owned(), to));
                self.run_ops(label, steps, Refresh::All, OpKind::plain(), ctx);
            }
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((format!("{label}: {e}"), true)))
            }
        }
    }

    /// Puts one branch back at `commit` (an entry's old tip), as an entry of its own.
    pub(in crate::repo) fn put_branch_back(
        &mut self,
        branch: &str,
        commit: &str,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        let now = self.ref_state();
        let mut target = now.clone();
        target.branches.insert(branch.to_owned(), commit.to_owned());
        let label = format!("Put {branch} back at {}", crate::repo::short(commit));
        match snapshot::restore_steps(&now, &target, &self.other_worktree_branches()) {
            Ok(steps) => self.run_ops(&label, steps, Refresh::All, OpKind::plain(), ctx),
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((format!("{label}: {e}"), true)))
            }
        }
    }

    /// Makes a new branch at `commit`, named after `branch` and the commit (with a number when
    /// taken).
    pub(in crate::repo) fn branch_at_old_tip(
        &mut self,
        branch: &str,
        commit: &str,
        ctx: &egui::Context,
    ) {
        let name = self.free_branch_name(&format!("{branch}-was-{}", crate::repo::short(commit)));
        let undo = Undo::Custom {
            undo: crate::repo::ops(&[&["branch", "-D", &name]]),
            redo: crate::repo::ops(&[&["branch", &name, commit]]),
        };
        self.run_undoable(
            &format!("Create branch {name}"),
            crate::repo::ops(&[&["branch", &name, commit]]),
            Refresh::All,
            undo,
            ctx,
        );
    }

    /// `base`, or `base-2`, `base-3`… whichever no branch has.
    pub(in crate::repo) fn free_branch_name(&self, base: &str) -> String {
        let taken = |n: &str| self.refs.local.iter().any(|b| b.name == n);
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|n| !taken(n))
            .unwrap_or_else(|| base.to_owned())
    }

    /// The branches checked out in this repository's other worktrees.
    fn other_worktree_branches(&self) -> Vec<String> {
        self.worktrees
            .iter()
            .filter(|w| !w.current)
            .filter_map(|w| w.branch.clone())
            .collect()
    }

    /// Forgets the entries older than the newest `keep` (all of them with 0), but those leading
    /// to the current one, and lets go of their commits.
    pub(in crate::repo) fn forget_timeline(&mut self, keep: usize, ctx: &egui::Context) {
        if keep == 0 {
            let all: Vec<u64> = self.timeline.entries.iter().map(|e| e.id).collect();
            self.timeline.forget(&all);
        } else {
            self.timeline.forget_older(keep);
        }
        self.save_timeline(ctx);
        self.release_dropped(ctx);
    }
}
