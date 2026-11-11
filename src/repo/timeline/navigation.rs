//! Timeline undo, redo and guarded branch restoration.

use super::*;

impl RepoTab {
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
        let again =
            crate::repo::Action::Timeline(crate::repo::timeline::view::TimelineAct::Jump(to));
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
    pub(in crate::repo::timeline::tab) fn timeline_unsettled(
        &mut self,
        what: &str,
        again: crate::repo::Action,
    ) -> bool {
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
    pub(in crate::repo::timeline::tab) fn without_elsewhere(
        &self,
        before: &RefState,
        after: &RefState,
    ) -> RefState {
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
    pub(in crate::repo::timeline::tab) fn restore(
        &mut self,
        label: &str,
        target: &RefState,
        to: Option<u64>,
        ctx: &egui::Context,
    ) {
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
    pub(in crate::repo::timeline::tab) fn other_worktree_branches(&self) -> Vec<String> {
        self.worktrees
            .iter()
            .filter(|w| !w.current)
            .filter_map(|w| w.branch.clone())
            .collect()
    }
}
