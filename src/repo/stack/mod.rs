//! Stacked branches in the tab: the layers read after each reload, the stacks they make in
//! the sidebar and the graph's tooltips, and _This branch is based on…_, which records a
//! branch's parent (see [`crate::git::stack`]).

mod dialog;
mod graph;
mod model;
mod publish;
mod restack;
mod section;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;

use crate::git::stack::{self, Layer, Recorded};

use super::{RepoTab, Undo};
pub(crate) use dialog::BasedOn;
pub(crate) use model::{Tree, chain, dependents, looped, to_restack, trees};

/// What the views ask of stacks.
pub(crate) enum StackAct {
    /// Open _This branch is based on…_ for the branch.
    OpenBasedOn(String),
    /// The parent chosen in the dialog.
    PickParent(String),
    /// Record the branch as based on the parent.
    SetParent {
        branch: String,
        parent: String,
    },
    /// Take the branch out of its stack.
    RemoveParent(String),
    CloseDialog,
    /// Preview restacking the layers on the branch, or (`None`) every layer that needs it.
    OpenRestack(Option<String>),
    /// Restack as previewed.
    StartRestack,
    CloseRestack,
    /// Go on with the restack that stopped.
    ResumeRestack,
    /// Give up the restack that stopped: every branch back where it was.
    AbortRestack,
    /// Preview publishing the pull requests of the stack the branch is in.
    OpenPublish(String),
    /// Open new pull requests as drafts, or not.
    PublishDraft(bool),
    /// The title of the new pull request of the layer at that place in the preview.
    PublishTitle(usize, String),
    /// Publish as previewed.
    StartPublish,
    ClosePublish,
}

/// The tab's stacks.
#[derive(Default)]
pub(crate) struct Stacks {
    /// What the config records, as last read.
    pub recorded: BTreeMap<String, Recorded>,
    /// The layers, as last read.
    pub layers: Vec<Layer>,
    /// Why they couldn't be read, the last time.
    pub error: Option<String>,
    read_error: Option<ReadError>,
    /// Numbers the reads: only the latest one's answer is taken.
    read: u64,
    pub dialog: Option<BasedOn>,
    /// The restack that stopped (on conflicts), as kept in the config.
    pub pending: Option<stack::restack::Pending>,
    /// The restack being previewed.
    pub preview: Option<restack::RestackPreview>,
    /// The pull requests of a stack being previewed.
    pub publish: Option<publish::PublishPreview>,
}

impl Stacks {
    /// The restack stopped in this worktree's rebase (not in another worktree's), if one is.
    pub fn restack_here(&self) -> Option<&stack::restack::Pending> {
        self.pending.as_ref().filter(|p| p.at.is_none())
    }

    /// Whether one of its dialogs is open (the keys are left to it).
    pub fn modal_open(&self) -> bool {
        self.dialog.is_some() || self.preview.is_some() || self.publish.is_some()
    }

    /// The layer `branch` is, if it is one.
    pub fn layer(&self, branch: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.branch == branch)
    }
}

/// Why `branch` can't be based on `parent`: that would make a loop.
fn loop_problem(branch: &str, parent: &str) -> String {
    if branch == parent {
        crate::i18n::message(
            "repo-workflows-stack-loop-problem-27b30746",
            &[("branch", (branch).into())],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-stack-loop-problem-bc8dbf35",
            &[("parent", (parent).into()), ("branch", (branch).into())],
        )
    }
}

impl RepoTab {
    /// Reads the recorded parents and the state of each layer in the background.
    pub(super) fn read_stacks(&mut self, ctx: &egui::Context) {
        self.stacks.read += 1;
        let read = self.stacks.read;
        let path = self.path.clone();
        let local: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        self.then(
            ctx,
            move || {
                let (recorded, pending) = stack::read_all(&path)?;
                // Most repositories have none: no more git to run then.
                let layers = if recorded.is_empty() {
                    Vec::new()
                } else {
                    stack::layers(&path, &recorded, &local)?
                };
                Ok::<_, String>((recorded, layers, pending))
            },
            move |tab, found, _| {
                if tab.stacks.read != read {
                    return;
                }
                match found {
                    Ok((recorded, layers, pending)) => {
                        tab.stacks.recorded = recorded;
                        tab.stacks.layers = layers;
                        tab.stacks.pending = pending;
                        tab.stacks.error = None;
                    }
                    Err(e) => tab.stacks.remember_read_error(e),
                }
            },
        );
    }

    pub(super) fn stack_act(&mut self, act: StackAct, ctx: &egui::Context) {
        match act {
            StackAct::OpenBasedOn(branch) => self.open_based_on(branch, ctx),
            StackAct::PickParent(parent) => {
                if let Some(dialog) = &mut self.stacks.dialog {
                    dialog.choice = parent;
                }
            }
            StackAct::SetParent { branch, parent } => {
                self.stacks.dialog = None;
                self.set_parent(branch, parent, ctx);
            }
            StackAct::RemoveParent(branch) => self.remove_parent(branch, ctx),
            StackAct::CloseDialog => self.stacks.dialog = None,
            StackAct::OpenRestack(from) => self.open_restack(from.as_deref(), ctx),
            StackAct::StartRestack => self.start_restack(ctx),
            StackAct::CloseRestack => self.stacks.preview = None,
            StackAct::ResumeRestack => self.resume_restack(ctx),
            StackAct::AbortRestack => self.abort_restack(ctx),
            StackAct::OpenPublish(branch) => self.open_publish(&branch, ctx),
            StackAct::PublishDraft(draft) => {
                if let Some(p) = &mut self.stacks.publish {
                    p.draft = draft;
                }
            }
            StackAct::PublishTitle(i, title) => {
                let plans = self.stacks.publish.as_mut().and_then(|p| p.plans.as_mut());
                if let Some(Ok(plans)) = plans
                    && let Some(plan) = plans.get_mut(i)
                {
                    plan.title = title;
                }
            }
            StackAct::StartPublish => self.start_publish(ctx),
            StackAct::ClosePublish => self.stacks.publish = None,
        }
    }

    /// Records `branch` as based on `parent`, at the commit it forks from it. What is recorded
    /// now (for the undo, and to refuse a loop) and the fork point are read in the background
    /// first: the last read of the stacks may be older than an operation just run. Undoable.
    fn set_parent(&mut self, branch: String, parent: String, ctx: &egui::Context) {
        if let Some(problem) = self.parent_problem(&branch, &parent) {
            self.status = Some(crate::repo::Notice::plain((problem, true)));
            return;
        }
        let path = self.path.clone();
        let (b, p) = (branch.clone(), parent.clone());
        self.then(
            ctx,
            move || {
                let recorded = stack::recorded(&path)?;
                if stack::would_loop(&recorded, &b, &p) {
                    return Err(loop_problem(&b, &p));
                }
                let base = stack::fork_point(&path, &b, &p)?;
                Ok((recorded.get(&b).cloned(), base))
            },
            move |tab, found, ctx| match found {
                Ok((before, base)) => {
                    let steps = stack::set_parent_ops(&branch, &parent, &base);
                    tab.run_stack_op(
                        format!("Base {branch} on {parent}"),
                        steps,
                        stack::restore_ops(&branch, before.as_ref()),
                        ctx,
                    );
                }
                Err(e) => tab.status = Some(crate::repo::Notice::plain((e, true))),
            },
        );
    }

    /// Why `branch` can't be based on `parent`, if it can't (as far as the last read knows).
    fn parent_problem(&self, branch: &str, parent: &str) -> Option<String> {
        if parent.is_empty() {
            return Some(crate::i18n::message(
                "repo-workflows-stack-parent-problem-61a1cff0",
                &[("branch", (branch).into())],
            ));
        }
        if !self.refs.local.iter().any(|b| b.name == parent) {
            return Some(crate::i18n::message(
                "repo-workflows-stack-parent-problem-f7032e38",
                &[("parent", (parent).into())],
            ));
        }
        if stack::would_loop(&self.stacks.recorded, branch, parent) {
            return Some(loop_problem(branch, parent));
        }
        None
    }

    /// Takes `branch` out of its stack, what it is recorded as based on read first (for the
    /// undo). Undoable.
    fn remove_parent(&mut self, branch: String, ctx: &egui::Context) {
        let path = self.path.clone();
        let b = branch.clone();
        self.then(
            ctx,
            move || stack::recorded(&path).map(|r| r.get(&b).cloned()),
            move |tab, found, ctx| match found {
                Ok(before) => tab.run_stack_op(
                    format!("Remove {branch} from its stack"),
                    stack::unset_parent_ops(&branch),
                    stack::restore_ops(&branch, before.as_ref()),
                    ctx,
                ),
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Couldn't read the stacks: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    /// Runs `steps` as an undoable operation, once the background read before it is in;
    /// says so rather than drop it when another operation started meanwhile.
    fn run_stack_op(
        &mut self,
        label: String,
        steps: Vec<Vec<String>>,
        undo: Vec<Vec<String>>,
        ctx: &egui::Context,
    ) {
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                format!("{label}: run it again once {busy} is done"),
                true,
            )));
            return;
        }
        self.run_undoable(
            &label,
            steps.clone(),
            super::Refresh::All,
            Undo::Custom { undo, redo: steps },
            ctx,
        );
    }

    /// The line a branch's tooltip gets when it is a layer of a stack.
    pub(super) fn stack_line(&self, branch: &str) -> Option<String> {
        let layer = self.stacks.layer(branch)?;
        let mut line = crate::i18n::message(
            "repo-workflows-stack-stack-line-559ed9fe",
            &[(
                "chain-self-stacks-layers-branch--1",
                (chain(&self.stacks.layers, branch).join(" → "))
                    .clone()
                    .into(),
            )],
        );
        if layer.parent_missing {
            line.push_str(&crate::i18n::message(
                "repo-workflows-stack-stack-line-e4355cbd",
                &[("layer-parent-1", (layer.parent).clone().into())],
            ));
        } else if layer.needs_restack() {
            line.push_str(&crate::i18n::message(
                "repo-workflows-stack-stack-line-c5c1ffb4",
                &[("layer-parent-1", (layer.parent).clone().into())],
            ));
        }
        Some(line)
    }
}

#[derive(Clone)]
struct ReadError {
    raw: String,
    detail: String,
}
impl Stacks {
    fn remember_read_error(&mut self, detail: String) {
        let e = &detail;
        let raw = format!("Couldn't read the stacks: {e}");
        self.error = Some(raw.clone());
        self.read_error = Some(ReadError { raw, detail });
    }
    fn display_read_error(&self, raw: &str) -> String {
        match &self.read_error {
            Some(receipt) if receipt.raw == raw => crate::i18n::message(
                "repo-workflows-stack-read-stacks-4272623a",
                &[("e", receipt.detail.as_str().into())],
            ),
            _ => raw.to_owned(),
        }
    }
}

#[cfg(test)]
mod fluent_r1_read_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_stack_cached_read_error_preserves_raw_detail_and_generation()
    -> Result<(), String> {
        let detail = "provider; ;\nHistory worker stopped: {$x} 日本語";
        crate::i18n::set_language("de");
        let mut state = Stacks::default();
        state.remember_read_error(detail.into());
        let raw = state.error.clone().ok_or("missing test value")?;
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            let shown = state.display_read_error(&raw);
            assert!(shown.contains(detail));
            assert_eq!(shown == raw, lang == "en-US");
            assert_eq!(state.error.as_ref(), Some(&raw));
        }
        state.read_error = None;
        crate::i18n::set_language("de");
        assert_eq!(state.display_read_error(&raw), raw);
        crate::i18n::set_language("en-US");

        Ok(())
    }
}
