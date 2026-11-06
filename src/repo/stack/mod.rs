//! Stacked branches in the tab: the layers read after each reload, the stacks they make in
//! the sidebar and the graph's tooltips, and _This branch is based on…_, which records a
//! branch's parent (see [`crate::git::stack`]).

mod dialog;
mod model;
mod section;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;

use crate::git::stack::{self, Layer, Recorded};

use super::{RepoTab, Undo};
pub(crate) use dialog::BasedOn;
pub(crate) use model::{Tree, chain, looped, trees};

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
    /// Numbers the reads: only the latest one's answer is taken.
    read: u64,
    pub dialog: Option<BasedOn>,
}

impl Stacks {
    /// The layer `branch` is, if it is one.
    pub fn layer(&self, branch: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.branch == branch)
    }
}

/// Why `branch` can't be based on `parent`: that would make a loop.
fn loop_problem(branch: &str, parent: &str) -> String {
    if branch == parent {
        format!("{branch} can't be based on itself.")
    } else {
        format!("{parent} is based on {branch}: {branch} can't be based on it too.")
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
                let recorded = stack::recorded(&path)?;
                // Most repositories have none: no more git to run then.
                let layers = if recorded.is_empty() {
                    Vec::new()
                } else {
                    stack::layers(&path, &recorded, &local)?
                };
                Ok::<_, String>((recorded, layers))
            },
            move |tab, found, _| {
                if tab.stacks.read != read {
                    return;
                }
                match found {
                    Ok((recorded, layers)) => {
                        tab.stacks.recorded = recorded;
                        tab.stacks.layers = layers;
                        tab.stacks.error = None;
                    }
                    Err(e) => tab.stacks.error = Some(format!("Couldn't read the stacks: {e}")),
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
        }
    }

    /// Records `branch` as based on `parent`, at the commit it forks from it. What is recorded
    /// now (for the undo, and to refuse a loop) and the fork point are read in the background
    /// first: the last read of the stacks may be older than an operation just run. Undoable.
    fn set_parent(&mut self, branch: String, parent: String, ctx: &egui::Context) {
        if let Some(problem) = self.parent_problem(&branch, &parent) {
            self.status = Some((problem, true));
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
                Err(e) => tab.status = Some((e, true)),
            },
        );
    }

    /// Why `branch` can't be based on `parent`, if it can't (as far as the last read knows).
    fn parent_problem(&self, branch: &str, parent: &str) -> Option<String> {
        if parent.is_empty() {
            return Some(format!("Pick the branch {branch} is based on."));
        }
        if !self.refs.local.iter().any(|b| b.name == parent) {
            return Some(format!("{parent} is not a local branch."));
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
                Err(e) => tab.status = Some((format!("Couldn't read the stacks: {e}"), true)),
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
            self.status = Some((format!("{label}: run it again once {busy} is done"), true));
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
        let mut line = format!("Stack: {}", chain(&self.stacks.layers, branch).join(" → "));
        if layer.parent_missing {
            line.push_str(&format!(" · {} is gone", layer.parent));
        } else if layer.needs_restack() {
            line.push_str(&format!(" · {} moved on: needs restacking", layer.parent));
        }
        Some(line)
    }
}
