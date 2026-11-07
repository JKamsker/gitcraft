//! The hotfix flow: _Start hotfix from v2.3.1_ (a tag's menu) makes `hotfix/2.3.2` there, and
//! _Finish hotfix…_ (the branch's menu, the palette) tags it, merges it back into the main branch
//! (and the development one, if used), deletes it and pushes it all, as one step shown before it
//! runs and undone with Ctrl+Z.

pub(crate) mod plan;
#[cfg(test)]
mod tests;
mod view;

use egui::Context;

use super::{Dialog, Refresh, RepoTab, Undo};
use crate::release::notes;

/// The dialog that finishes a hotfix.
pub(crate) struct FinishView {
    /// `None` while it is read.
    plan: Option<Result<plan::Finish, String>>,
    choices: plan::Choices,
}

/// What the hotfix flow is asked to do.
pub(crate) enum HotfixAct {
    /// Ask for the branch of a hotfix from the tag (by name, at its commit).
    Start {
        tag: String,
        target: String,
    },
    /// Read what finishing the hotfix checked out starts from, and show it.
    OpenFinish,
    Finish,
    Close,
}

struct Running {
    label: String,
    plan: plan::Finish,
    choices: plan::Choices,
    push: Option<Vec<String>>,
}

/// The hotfix's state in the tab.
#[derive(Default)]
pub(crate) struct Hotfix {
    pub(crate) finish: Option<FinishView>,
    /// The finish under way: its operation's label, what it was planned from and the push
    /// that follows it once it has gone through.
    running: Option<Running>,
    /// The branches' names, read with the repository.
    prefix: Option<String>,
}

impl RepoTab {
    pub(super) fn hotfix_act(&mut self, act: HotfixAct, ctx: &Context) {
        match act {
            HotfixAct::Start { tag, target } => {
                let prefix = self.hotfix_prefix();
                let Some(name) = plan::start_name(&tag, &prefix) else {
                    self.status = Some((
                        format!("{tag} isn't a version to start a hotfix from"),
                        true,
                    ));
                    return;
                };
                let mut dialog = Dialog::create_branch(&target, &tag);
                if let Dialog::CreateBranch { name: field, .. } = &mut dialog {
                    *field = name;
                }
                self.dialog = Some(dialog);
            }
            HotfixAct::OpenFinish => {
                if self.busy_op.is_some() {
                    return;
                }
                self.hotfix.finish = Some(FinishView {
                    plan: None,
                    choices: plan::Choices {
                        tag: String::new(),
                        notes: String::new(),
                        develop: true,
                        delete: true,
                        push: true,
                        sign: false,
                    },
                });
                let repo = self.path.clone();
                let remote = self.default_remote();
                let hosting = self.hosting.clone();
                self.forge_then(
                    ctx,
                    move || plan::plan(&repo, remote),
                    move |tab, plan, _| {
                        let Some(view) = &mut tab.hotfix.finish else {
                            return;
                        };
                        if let Ok(p) = &plan {
                            let link =
                                |n: u64| hosting.as_ref().map(|h| crate::release::pr_url(h, n));
                            view.choices = plan::Choices {
                                tag: p.tag.clone(),
                                notes: notes::notes(&p.changes, &link),
                                develop: p.develop.is_some(),
                                delete: true,
                                push: p.remote.is_some(),
                                sign: p.sign,
                            };
                        }
                        view.plan = Some(plan);
                    },
                );
            }
            HotfixAct::Finish => self.finish_hotfix(ctx),
            HotfixAct::Close => self.hotfix.finish = None,
        }
    }

    /// The prefix of hotfix branches here (`hotfix/`, or git-flow's).
    pub(super) fn hotfix_prefix(&self) -> String {
        self.hotfix
            .prefix
            .clone()
            .unwrap_or_else(|| "hotfix/".to_owned())
    }

    /// Each frame: reads git-flow's prefix after a reload (it rarely changes; once per read).
    pub(super) fn hotfix_frame(&mut self, ctx: &Context) {
        if self.hotfix.prefix.is_some() || self.history_gen == 0 {
            return;
        }
        self.hotfix.prefix = Some("hotfix/".to_owned());
        let repo = self.path.clone();
        self.then(
            ctx,
            move || plan::names(&repo).prefix,
            |tab, prefix, _| tab.hotfix.prefix = Some(prefix),
        );
    }

    fn finish_hotfix(&mut self, ctx: &Context) {
        let Some(view) = &self.hotfix.finish else {
            return;
        };
        let Some(Ok(plan)) = &view.plan else { return };
        let c = view.choices.clone();
        if plan.problem(&c).is_some() || self.busy_op.is_some() {
            return;
        }
        let label = format!("Finish hotfix {}", c.tag.trim());
        let steps = plan.steps(&c);
        let undo = plan.undo(&c);
        self.hotfix.running = Some(Running {
            label: label.clone(),
            plan: plan.clone(),
            choices: c.clone(),
            push: plan.push(&c),
        });
        self.hotfix.finish = None;
        self.run_undoable(
            &label,
            steps.clone(),
            Refresh::All,
            Undo::Custom { undo, redo: steps },
            ctx,
        );
    }

    /// Takes in how an operation ended. Once the hotfix is finished here, redo is made to put
    /// back these very merges and tag, and it is pushed (a push of its own, so that a refused
    /// push leaves what was done here undoable, and signing in pushes again). Stopped part-way,
    /// what it did is made undoable.
    pub(super) fn hotfix_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &Context,
    ) {
        let Some(run) = self.hotfix.running.take_if(|r| r.label == label) else {
            return;
        };
        let repo = self.path.clone();
        if result.is_ok() {
            let (finish, choices) = (run.plan.clone(), run.choices.clone());
            let label = run.label.clone();
            let entry_label = label.clone();
            self.then(
                ctx,
                move || plan::now(&repo, &finish, &choices),
                move |tab, now, ctx| {
                    let Some(tag) = now.tag else { return };
                    let redo = run.plan.redo(&run.choices, &now.branches, &tag);
                    tab.update_recent_redo(&entry_label, redo, ctx);
                },
            );
            if let Some(push) = run.push {
                let tag = label.trim_start_matches("Finish hotfix ").to_owned();
                self.run_ops(
                    &format!("Push hotfix {tag}"),
                    vec![push],
                    Refresh::All,
                    super::OpKind::plain(),
                    ctx,
                );
            }
            return;
        }
        let (finish, choices) = (run.plan.clone(), run.choices.clone());
        self.then(
            ctx,
            move || plan::now(&repo, &finish, &choices),
            move |tab, now, ctx| {
                let Some(undo) = plan::recover(&run.plan, &run.choices, &now) else {
                    return;
                };
                let redo = run.plan.steps(&run.choices);
                tab.push_undo(
                    super::state::UndoEntry {
                        id: 0,
                        agent: None,
                        label: format!("{} (stopped part-way)", run.label),
                        undo,
                        redo,
                        heads: None,
                    },
                    ctx,
                );

                if let Some((text, true)) = &mut tab.status {
                    text.push_str(" — Ctrl+Z puts back what it did");
                }
            },
        );
    }
}
