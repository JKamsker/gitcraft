//! Folding the staged changes into an older commit of the checked-out branch: from a commit's
//! menu, or the picker beside _Amend_ in the commit box. A dialog shows the commit and those
//! after it that get rewritten (and which are pushed); then the changes are committed as
//! `fixup! <summary>`, and either kept so or melded into the commit by a rebase without the
//! editor. One undo puts the branch back with the changes staged again; a rebase stopped on a
//! conflict goes on in the usual conflict flow, and aborting it goes back to before the fold.

pub(crate) mod plan;
#[cfg(test)]
mod tests;

use std::borrow::Borrow;

use egui::{RichText, Ui};

use super::quick_edit::first_parents;
use super::{Action, Dialog, OpKind, Refresh, RepoTab, abort_action, short};
use crate::git::{self, Commit, InProgress};
use crate::theme::Tokens;
pub(crate) use plan::Plan;
pub(super) use plan::Stopped;

/// How many commits of the branch the commit box's picker offers.
const PICKER_COMMITS: usize = 15;

/// A fold running or stopped on a conflict.
#[derive(Default)]
pub(super) struct FoldState {
    /// The fold running now, until it ends.
    running: Option<Stopped>,
    /// The fold whose rebase stopped, until it is continued to the end or aborted.
    stopped: Option<Stopped>,
    /// The fold being taken back because its result lost some of the staged changes.
    putting_back: Option<Stopped>,
}

impl RepoTab {
    /// Why nothing can be folded now, whatever the commit, as the last read saw it.
    fn fold_blocked(&self) -> Option<String> {
        if self.refs.head.is_empty() {
            return Some("There are no commits yet".to_owned());
        }
        plan::blocked(
            self.work.staged.len(),
            self.refs.current.as_deref(),
            self.work.in_progress,
        )
    }

    /// Why the staged changes can't go into `hash` now.
    fn fold_refusal(&self, line: &[&Commit], hash: &str) -> Option<String> {
        self.fold_blocked()
            .or_else(|| plan::refusal(line, hash).map(str::to_owned))
    }

    /// _Fold staged changes into this commit_ in a commit's menu, disabled with the reason.
    pub(super) fn fold_item(&self, ui: &mut Ui, hash: &str, idle: bool, acts: &mut Vec<Action>) {
        let line = first_parents(&self.commits, &self.index, &self.refs.head);
        let refused = self.fold_refusal(&line, hash);
        let button = ui
            .add_enabled(
                idle && refused.is_none(),
                egui::Button::new("Fold staged changes into this commit"),
            )
            .on_hover_text(
                "Put the staged changes into this commit, rewriting the commits after it",
            );
        let button = match &refused {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        if button.clicked() {
            acts.push(Action::FoldInto(hash.to_owned()));
        }
    }

    /// _Fold…_ beside _Amend last commit_: the latest commits of the branch to fold the staged
    /// changes into. `amending`: that box is ticked, which the fold would not follow.
    pub(super) fn fold_picker(&self, ui: &mut Ui, amending: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let blocked = self.fold_blocked().or_else(|| {
            amending.then(|| "Untick Amend last commit to fold into an older commit".to_owned())
        });
        let button = ui
            .add_enabled(idle && blocked.is_none(), egui::Button::new("Fold…"))
            .on_hover_text("Fold the staged changes into an older commit of this branch");
        let button = match &blocked {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        egui::Popup::menu(&button)
            .style(crate::theme::menu_style)
            .show(|ui| {
                // Walked only while the picker is open.
                let line = first_parents(&self.commits, &self.index, &self.refs.head);
                ui.set_max_width(360.0);
                ui.label(
                    RichText::new("Fold the staged changes into:")
                        .color(t.text_faint)
                        .small(),
                );
                for commit in line.iter().take(PICKER_COMMITS) {
                    let refused = self.fold_refusal(&line, &commit.hash);
                    let text = format!("{}  {}", short(&commit.hash), commit.summary);
                    let item =
                        ui.add_enabled(refused.is_none(), egui::Button::new(text).truncate());
                    let item = match &refused {
                        Some(why) => item.on_disabled_hover_text(why),
                        None => item,
                    };
                    if item.clicked() {
                        acts.push(Action::FoldInto(commit.hash.clone()));
                    }
                }
                if line.len() > PICKER_COMMITS {
                    ui.label(
                        RichText::new("Older commits: right-click them in the graph")
                            .color(t.text_faint)
                            .small(),
                    );
                }
            });
    }

    /// Reads what folding into `hash` rewrites (in the background), then asks with the dialog.
    pub(super) fn fold_into(&mut self, hash: String, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::fold_read(&path, &hash).and_then(Plan::from_read),
            |tab, plan, _| match plan {
                Ok(plan) if tab.dialog.is_none() => {
                    tab.dialog = Some(Dialog::Fold(Box::new(plan)));
                }
                Ok(_) => {}
                Err(e) => tab.status = Some((format!("Fold staged changes: {e}"), true)),
            },
        );
    }

    /// Runs the fold the dialog confirmed.
    pub(super) fn run_fold(&mut self, plan: Plan, ctx: &egui::Context) {
        if let Some(busy) = &self.busy_op {
            self.status = Some((
                format!(
                    "{}: {busy} is still running, try again when it's done",
                    plan.label()
                ),
                true,
            ));
            return;
        }
        let label = plan.label();
        if plan.keep_fixup {
            let (steps, undo) = plan.keep_steps();
            self.run_undoable(&label, steps, Refresh::All, undo, ctx);
            return;
        }
        match plan.editor() {
            Ok(editor) => self.with_rebase_command(editor, ctx, move |tab, command, ctx| {
                tab.fold_ready(&plan, command, ctx);
            }),
            Err(e) => self.status = Some((format!("{label}: {e}"), true)),
        }
    }

    /// Runs the fold once its rebase's todo list is written (on another machine, in the
    /// background).
    fn fold_ready(
        &mut self,
        plan: &Plan,
        command: Result<(Vec<String>, String), String>,
        ctx: &egui::Context,
    ) {
        let label = plan.label();
        match command {
            // Started while something else ran: say so rather than drop it.
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some((
                    format!(
                        "{label}: {} is still running, try again when it's done",
                        self.busy_op.as_deref().unwrap_or_default()
                    ),
                    true,
                ));
            }
            Ok((rebase, _)) => {
                let (steps, undo) = plan.fold_steps(rebase);
                self.fold.running = Some(plan.stopped());
                self.run_undoable(&label, steps, Refresh::All, undo, ctx);
            }
            Err(e) => self.status = Some((format!("{label}: {e}"), true)),
        }
    }

    /// Takes in how an operation ended, for a fold: one that failed may have stopped in its
    /// rebase, to be continued or aborted; continued to the end or aborted, it is over.
    /// Called before the reload after the operation starts.
    pub(super) fn fold_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        let ok = result.is_ok();
        if let Some(back) = self
            .fold
            .putting_back
            .take_if(|r| put_back_label(r) == label)
        {
            self.status = Some(match result {
                Ok(_) => (
                    format!(
                        "{}: nothing was changed. The staged changes undo what a later commit \
                         does, so they would have been lost in the commit; they are staged as \
                         before.",
                        back.label
                    ),
                    true,
                ),
                Err(e) => (
                    format!(
                        "{}: the result lost some of the staged changes, and going back failed \
                         ({e}). They are all in ORIG_HEAD, the fixup! commit.",
                        back.label
                    ),
                    true,
                ),
            });
            return;
        }
        if let Some(mut running) = self.fold.running.take_if(|r| r.label == label) {
            if let Err(e) = result
                && plan::lost_in_rebase(e)
            {
                // Done, but not as staged: taken back at once, before anything else runs.
                let steps = running.put_back_steps();
                let back = put_back_label(&running);
                self.fold.putting_back = Some(running);
                self.run_ops(&back, steps, Refresh::All, OpKind::plain(), ctx);
                return;
            }
            if let Err(e) = result {
                // The changes are in the fixup commit the rebase stopped on, not lost: say how
                // to go on rather than only that it failed.
                if e.contains("CONFLICT") || e.contains("could not apply") {
                    self.status = Some((
                        format!(
                            "{label} stopped on a conflict: resolve it and Continue rebase, or \
                             Abort to go back with the changes staged"
                        ),
                        true,
                    ));
                }
                // The status read the reload after this starts tells whether it stopped.
                running.read = Some(self.reads + 1);
                self.fold.stopped = Some(running);
            }
            return;
        }
        let over = |s: &Stopped| label == s.label || label == abort_label(s);
        if ok && self.fold.stopped.as_ref().is_some_and(over) {
            self.fold.stopped = None;
        }
    }

    /// Takes in the repository read numbered `read` (its refs and history already in), with
    /// `state` in progress: a stopped fold whose rebase isn't going on any more (finished or
    /// aborted in the terminal, or never started) is forgotten. When it left its fixup commit
    /// on top (the rebase failed before it began, or was aborted elsewhere), that commit gets
    /// an undo of its own, which stages the changes again.
    pub(super) fn fold_loaded(&mut self, read: u64, state: Option<InProgress>) {
        if !self
            .fold
            .stopped
            .as_ref()
            .is_some_and(|s| s.over(read, state))
        {
            return;
        }
        let Some(stopped) = self.fold.stopped.take() else {
            return;
        };
        let head = self.refs.head.clone();
        let on_top = self
            .history
            .iter()
            .find(|c| c.hash == head)
            .is_some_and(|c| c.parents.first() == Some(&stopped.before));
        if !on_top || self.refs.current.as_deref() != Some(stopped.branch.as_str()) {
            return;
        }
        self.undo_stack.push(stopped.leftover_undo(&head));
        self.redo_stack.clear();
        self.status = Some((
            format!(
                "{}: the rebase didn't run, so the changes are in a commit on top. Ctrl+Z \
                 stages them again.",
                stopped.label
            ),
            true,
        ));
    }

    /// The fold stopped in the rebase going on now: the branch is still the fixup commit on top
    /// of where the fold started (and not a rebase started since in the terminal).
    pub(super) fn fold_stopped_here(&self, state: InProgress) -> Option<&Stopped> {
        let stopped = self.fold.stopped.as_ref()?;
        let tip = self.refs.local.iter().find(|b| b.name == stopped.branch)?;
        let parent = self
            .index
            .get(&tip.target)
            .and_then(|&i| self.commits.get(i))
            .and_then(|c| Borrow::<Commit>::borrow(c).parents.first().cloned());
        (state == InProgress::Rebase && parent.as_deref() == Some(stopped.before.as_str()))
            .then_some(stopped)
    }

    /// Abort, asked first: for a fold stopped on a conflict, back to before the fold with the
    /// changes staged again.
    pub(super) fn abort_for(&self, state: InProgress) -> Action {
        let Some(stopped) = self.fold_stopped_here(state) else {
            return abort_action(state);
        };
        Action::Dialog(Dialog::confirm(
            &abort_label(stopped),
            "Stop the fold and go back to where it started? The branch is put back as it was, \
             with the changes staged again. Conflicts resolved so far are lost, and can't be \
             brought back."
                .to_owned(),
            "Abort",
            stopped.abort_steps(),
            None,
        ))
    }

    /// Continuing the rebase of a fold stopped on a conflict: its label and how it is undone
    /// (as the whole fold).
    pub(super) fn fold_continue(&self, state: InProgress) -> Option<(String, OpKind)> {
        self.fold_stopped_here(state)
            .map(|s| (s.label.clone(), s.continue_kind()))
    }
}

fn put_back_label(stopped: &Stopped) -> String {
    format!("Take back {}", stopped.label.replacen("Fold", "fold", 1))
}

fn abort_label(stopped: &Stopped) -> String {
    format!("Abort {}", stopped.label.replacen("Fold", "fold", 1))
}
