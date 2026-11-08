//! The tab's side of "Remove this, keep later work": working out what it takes out and plays
//! again, playing them again as an operation of its own (on the timeline, undoable), and
//! throwing away an attempt that stopped on a conflict.

use super::remove::{self, Outcome, Plan};
use crate::git;
use crate::repo::messages::{Msg, spawn_or};
use crate::repo::operations::expect_ref_at;
use crate::repo::{OpKind, Refresh, RepoTab, UndoEntry, ops, short};

/// Where the removal of an entry is, in the timeline view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemoveState {
    /// Working out what it takes out (for the entry).
    Planning(u64),
    Planned {
        plan: Plan,
        name: String,
    },
    Failed {
        id: u64,
        error: String,
    },
    Running {
        id: u64,
    },
    /// `confirm`: discarding the attempt (on a conflict) was asked, waiting to be confirmed.
    Finished {
        id: u64,
        outcome: Outcome,
        confirm: bool,
    },
}

impl RemoveState {
    /// The entry it is about.
    pub(crate) fn id(&self) -> u64 {
        match self {
            Self::Planning(id)
            | Self::Failed { id, .. }
            | Self::Running { id }
            | Self::Finished { id, .. } => *id,
            Self::Planned { plan, .. } => plan.id,
        }
    }
}

impl RepoTab {
    fn set_remove(&mut self, state: Option<RemoveState>) {
        if let Some(view) = &mut self.timeline_view {
            view.remove = state;
        }
    }

    fn remove_state(&self) -> Option<&RemoveState> {
        self.timeline_view.as_ref().and_then(|v| v.remove.as_ref())
    }

    /// Works out, in the background, what removing entry `id` takes out and plays again.
    pub(in crate::repo) fn ask_remove(&mut self, id: u64, ctx: &egui::Context) {
        let Some(entry) = self.timeline.get(id).cloned() else {
            return;
        };
        let now = remove::candidate(&entry)
            .and_then(|(branch, ..)| self.refs.local.iter().find(|b| b.name == branch))
            .map(|b| b.target.clone())
            .unwrap_or_default();
        self.set_remove(Some(RemoveState::Planning(id)));
        let path = self.path.clone();
        self.run_job(
            ctx,
            move || remove::plan(&path, &entry, &now),
            move |tab, plan, _| {
                // Asked about another entry since: not this one's answer.
                if tab.remove_state() != Some(&RemoveState::Planning(id)) {
                    return;
                }
                let state = match plan {
                    Ok(plan) => {
                        let name = tab.free_branch_name(&format!(
                            "{}-without-{}",
                            plan.branch,
                            short(&plan.new)
                        ));
                        RemoveState::Planned { plan, name }
                    }
                    Err(error) => RemoveState::Failed { id, error },
                };
                tab.set_remove(Some(state));
            },
        );
    }

    /// The name of the new branch, as typed.
    pub(in crate::repo) fn set_remove_name(&mut self, typed: String) {
        if let Some(view) = &mut self.timeline_view
            && let Some(RemoveState::Planned { name, .. }) = &mut view.remove
        {
            *name = typed;
        }
    }

    /// Plays the later commits again on a new branch, as an operation of its own.
    pub(in crate::repo) fn run_remove(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(RemoveState::Planned { plan, name }) = self.remove_state().cloned() else {
            return;
        };
        let name = name.trim().to_owned();
        if crate::repo::commands::refuse_option_like(&name, &mut self.status) {
            return;
        }
        if name.is_empty() || self.refs.local.iter().any(|b| b.name == name) {
            self.status = Some(crate::repo::Notice::plain((
                format!("Pick a name no branch has for the new branch (not “{name}”)"),
                true,
            )));
            return;
        }
        let label = format!(
            "Remove “{}” from {}, as {name}",
            self.entry_label(plan.id),
            plan.branch
        );
        self.busy_op = Some(label.clone());
        self.op_started = Some(std::time::Instant::now());
        self.timeline_op_started(false, Refresh::All);
        self.set_remove(Some(RemoveState::Running { id: plan.id }));
        let path = self.path.clone();
        let id = plan.id;
        let work = move || {
            git::run(Some(&path), &["check-ref-format", "--branch", &name]).map_err(|_| {
                crate::i18n::message(
                    "repo-workflows-timeline-run-remove-d0ee0326",
                    &[("name", (name).clone().into())],
                )
            })?;
            remove::replay(&path, &plan, &name)
        };
        self.run_job(ctx, work, move |tab, result, ctx| {
            tab.remove_done(&label, id, result, ctx);
        });
    }

    fn remove_done(
        &mut self,
        label: &str,
        id: u64,
        result: Result<Outcome, String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        self.op_started = None;
        let (undo, state, status) = match result {
            Ok(Outcome::Done { branch, tip }) => {
                let undo = UndoEntry {
                    agent: None,
                    id: 0,
                    label: label.to_owned(),
                    undo: [expect_ref_at(&format!("refs/heads/{branch}"), &tip)]
                        .into_iter()
                        .chain(ops(&[&["branch", "-D", "--", &branch]]))
                        .collect(),
                    redo: ops(&[&["branch", "--", &branch, &tip]]),
                    heads: None,
                };
                let status = crate::i18n::message(
                    "repo-workflows-timeline-remove-done-cd7d7755",
                    &[
                        ("label", (label).clone().into()),
                        ("branch", (branch).clone().into()),
                    ],
                );
                let outcome = Outcome::Done { branch, tip };
                let state = RemoveState::Finished {
                    id,
                    outcome,
                    confirm: false,
                };
                (Some(undo), state, (status, false))
            }
            Ok(outcome @ Outcome::Conflict { .. }) => {
                let status = crate::i18n::message(
                    "repo-workflows-timeline-remove-done-ff66751a",
                    &[("label", (label).clone().into())],
                );
                let state = RemoveState::Finished {
                    id,
                    outcome,
                    confirm: false,
                };
                (None, state, (status, true))
            }
            Err(error) => {
                let status = crate::i18n::message(
                    "repo-workflows-timeline-remove-done-0457f228",
                    &[
                        ("label", (label).clone().into()),
                        ("error", (error).clone().into()),
                    ],
                );
                (None, RemoveState::Failed { id, error }, (status, true))
            }
        };
        let ok = !matches!(state, RemoveState::Failed { .. });
        let stopped = matches!(
            state,
            RemoveState::Finished {
                outcome: Outcome::Conflict { .. },
                ..
            }
        );
        let entry_label = if stopped {
            crate::i18n::message(
                "repo-workflows-timeline-remove-done-7da61c9b",
                &[("label", (label).clone().into())],
            )
        } else {
            label.to_owned()
        };
        self.status = Some(crate::repo::Notice::plain(status));
        self.set_remove(Some(state));
        self.timeline_op_ended(&entry_label, &OpKind::plain(), undo, ok, Refresh::All, ctx);
        self.reload(ctx);
    }

    /// Throws away an attempt that stopped on a conflict: its worktree and its branch.
    pub(in crate::repo) fn discard_remove(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(RemoveState::Finished {
            outcome:
                Outcome::Conflict {
                    branch,
                    folder_arg,
                    stopped_at,
                    ..
                },
            confirm: true,
            ..
        }) = self.remove_state().cloned()
        else {
            return;
        };
        let label = format!("Discard {branch}");
        self.busy_op = Some(label.clone());
        self.op_started = Some(std::time::Instant::now());
        self.timeline_op_started(false, Refresh::All);
        let path = self.path.clone();
        self.run_job(
            ctx,
            move || remove::discard(&path, &folder_arg, &branch, &stopped_at),
            move |tab, result, ctx| {
                tab.busy_op = None;
                tab.op_started = None;
                let ok = result.is_ok();
                tab.status = Some(crate::repo::Notice::plain(match result {
                    Ok(()) => (format!("{label}: done"), false),
                    Err(e) => (format!("{label} failed: {e}"), true),
                }));
                if ok {
                    tab.set_remove(None);
                }
                tab.timeline_op_ended(&label, &OpKind::plain(), None, ok, Refresh::All, ctx);
                tab.reload(ctx);
            },
        );
    }

    /// Asks before discarding an attempt that stopped on a conflict (or takes the question back).
    pub(in crate::repo) fn confirm_discard(&mut self, ask: bool) {
        if let Some(view) = &mut self.timeline_view
            && let Some(RemoveState::Finished { confirm, .. }) = &mut view.remove
        {
            *confirm = ask;
        }
    }

    /// The label of entry `id`.
    fn entry_label(&self, id: u64) -> String {
        self.timeline
            .get(id)
            .map(|e| e.label.clone())
            .unwrap_or_default()
    }

    /// Runs `work` in the background, then `then` on the UI thread, also when `work` panics
    /// (with that as its error): the tab is busy until then.
    fn run_job<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        then: impl FnOnce(&mut RepoTab, Result<T, String>, &egui::Context) + Send + Clone + 'static,
    ) {
        let on_panic = then.clone();
        spawn_or(
            &self.tx,
            ctx,
            move |e| Msg::Then(Box::new(move |tab, ctx| on_panic(tab, Err(e), ctx))),
            move || {
                let found = work();
                Msg::Then(Box::new(move |tab, ctx| then(tab, found, ctx)))
            },
        );
    }
}
