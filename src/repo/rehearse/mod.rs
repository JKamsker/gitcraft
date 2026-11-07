//! Rehearse: a rebase or merge of the checked-out branch played through in a scratch
//! worktree, the test command run on the result, before doing it for real. Both can be
//! rehearsed and compared; applying one checks that neither branch moved meanwhile, then does
//! it as the menu would (into the undo journal, conflicts into the conflict editor).

mod report;
#[cfg(test)]
mod tests;
mod window;

use std::time::Instant;

use super::actions::BranchRef;
use super::messages::Msg;
use super::{Action, Dialog, RepoTab, merge_action, rebase_action};
use crate::git::{self, Rehearsal, RehearseKind, RehearseSpec, RunStop};

/// What the views ask of a rehearsal.
#[derive(Clone, Debug)]
pub(crate) enum RehearseAct {
    /// Open the dialog for the checked-out branch with `other` (its short name, remote-tracking
    /// when `remote`).
    Open {
        other: String,
        remote: bool,
    },
    /// Play these with `other`, testing the result with `command` when given.
    Start {
        other: String,
        remote: bool,
        kinds: Vec<RehearseKind>,
        command: Option<String>,
    },
    Stop,
    Close,
    /// Do the rehearsed operation for real.
    Apply(RehearseKind),
}

/// A rehearsal going on or played, until its window is closed.
pub(super) struct RehearseRun {
    id: u64,
    /// The branch, by short name.
    branch: String,
    other: String,
    remote: bool,
    command: Option<String>,
    kinds: Vec<RehearseKind>,
    /// Each kind's outcome, in `kinds`' order, as they come.
    results: Vec<(RehearseKind, Result<Rehearsal, String>)>,
    stop: RunStop,
    started: Instant,
    ended: bool,
    /// An apply is being checked.
    applying: bool,
}

/// A rehearsal still going when its tab closes is stopped.
impl Drop for RehearseRun {
    fn drop(&mut self) {
        if !self.ended {
            self.stop.stop();
        }
    }
}

/// The tab's rehearsal.
#[derive(Default)]
pub(super) struct Rehearsing {
    run: Option<RehearseRun>,
    runs: u64,
}

impl Rehearsing {
    pub(super) fn going(&self) -> bool {
        self.run.as_ref().is_some_and(|r| !r.ended)
    }
}

/// The menu entry that opens the dialog to rehearse with `other`.
pub(crate) fn rehearse_item(ui: &mut egui::Ui, acts: &mut Vec<Action>, other: BranchRef<'_>) {
    if ui
        .button("Rehearse merge or rebase…")
        .on_hover_text(
            "Plays it through in a scratch worktree first: where it stops, which files \
             conflict, whether the tests pass. Nothing of yours changes.",
        )
        .clicked()
    {
        acts.push(Action::Rehearse(RehearseAct::Open {
            other: other.name.to_owned(),
            remote: other.remote,
        }));
        ui.close();
    }
}

impl RepoTab {
    /// The palette's entry: rehearse with the branch pull requests go into.
    pub(super) fn rehearse_palette_items(&self) -> Vec<(String, String, Action)> {
        let (Some(current), Some(base)) = (self.refs.current.as_deref(), self.base_branch_name())
        else {
            return Vec::new();
        };
        if base == current || self.rehearsing.going() {
            return Vec::new();
        }
        let remote = !self.refs.local.iter().any(|b| b.name == base);
        vec![(
            format!("Rehearse merge or rebase of {current} with {base}…"),
            "rehearse dry run try preview".to_owned(),
            Action::Rehearse(RehearseAct::Open {
                other: base,
                remote,
            }),
        )]
    }

    pub(super) fn rehearse_act(&mut self, act: RehearseAct, ctx: &egui::Context) {
        match act {
            RehearseAct::Open { other, remote } => {
                let command = self.test_lane.command().unwrap_or_default().to_owned();
                self.dialog = Some(Dialog::Rehearse {
                    other,
                    remote,
                    rebase: true,
                    compare: true,
                    command,
                    focus: true,
                });
            }
            RehearseAct::Start {
                other,
                remote,
                kinds,
                command,
            } => self.start_rehearsal(other, remote, kinds, command, ctx),
            RehearseAct::Stop => {
                if let Some(run) = &self.rehearsing.run {
                    run.stop.stop();
                }
            }
            RehearseAct::Close => {
                // Not while an apply is being checked: its answer belongs to this window.
                let applying = self.rehearsing.run.as_ref().is_some_and(|r| r.applying);
                if !self.rehearsing.going() && !applying {
                    self.rehearsing.run = None;
                }
            }
            RehearseAct::Apply(kind) => self.apply_rehearsal(kind, ctx),
        }
    }

    fn start_rehearsal(
        &mut self,
        other: String,
        remote: bool,
        kinds: Vec<RehearseKind>,
        command: Option<String>,
        ctx: &egui::Context,
    ) {
        if self.rehearsing.going() {
            self.status = Some(("A rehearsal is going on already".to_owned(), true));
            return;
        }
        let Some(branch) = self.refs.current.clone() else {
            self.status = Some(("Check out a branch to rehearse with it".to_owned(), true));
            return;
        };
        let command = command.filter(|c| !c.trim().is_empty());
        self.rehearsing.runs += 1;
        let id = self.rehearsing.runs;
        let stop = RunStop::default();
        self.rehearsing.run = Some(RehearseRun {
            id,
            branch: branch.clone(),
            other: other.clone(),
            remote,
            command: command.clone(),
            kinds: kinds.clone(),
            results: Vec::new(),
            stop: stop.clone(),
            started: Instant::now(),
            ended: false,
            applying: false,
        });
        let full_other = BranchRef {
            name: &other,
            remote,
        }
        .full();
        let specs: Vec<RehearseSpec> = kinds
            .iter()
            .map(|&kind| RehearseSpec {
                kind,
                branch: BranchRef::local(&branch).full(),
                other: full_other.clone(),
                command: command.clone(),
            })
            .collect();
        let path = self.path.clone();
        let tx = self.tx.clone();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            for spec in specs {
                if stop.stopped() {
                    break;
                }
                let kind = spec.kind;
                let played = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    git::rehearse(&path, &spec, &stop)
                }))
                .unwrap_or_else(|_| Err("Internal error: the rehearsal stopped".to_owned()));
                let _ = tx.send(Msg::Then(Box::new(move |tab, _| {
                    tab.rehearsed(id, kind, played);
                })));
                wake.request_repaint();
            }
            let _ = tx.send(Msg::Then(Box::new(move |tab, _| tab.rehearsals_ended(id))));
            wake.request_repaint();
        });
    }

    fn rehearsed(&mut self, id: u64, kind: RehearseKind, played: Result<Rehearsal, String>) {
        if let Some(run) = self.rehearsing.run.as_mut().filter(|r| r.id == id) {
            run.results.push((kind, played));
        }
    }

    fn rehearsals_ended(&mut self, id: u64) {
        let name = self.name.clone();
        let Some(run) = self.rehearsing.run.as_mut().filter(|r| r.id == id) else {
            return;
        };
        run.ended = true;
        if run.started.elapsed() > std::time::Duration::from_secs(4) {
            self.finished
                .push((format!("{name} · rehearsal of {} done", run.branch), false));
        }
    }

    /// Checks that the branches are where they were rehearsed (and the branch still checked
    /// out), then does the operation as the menu would.
    fn apply_rehearsal(&mut self, kind: RehearseKind, ctx: &egui::Context) {
        let Some(run) = self
            .rehearsing
            .run
            .as_mut()
            .filter(|r| r.ended && !r.applying)
        else {
            return;
        };
        let Some(rehearsal) = run
            .results
            .iter()
            .find(|(k, _)| *k == kind)
            .and_then(|(_, r)| r.as_ref().ok())
            .cloned()
        else {
            return;
        };
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the operation going on to finish".to_owned(), true));
            return;
        }
        let work = &self.work;
        let changed = !work.staged.is_empty()
            || !work.conflicted.is_empty()
            || work.in_progress.is_some()
            || work.unstaged.iter().any(|f| f.area != git::Area::Untracked);
        if changed {
            self.status = Some((
                "Commit or stash your changes first: the rehearsal was of the commits alone"
                    .to_owned(),
                true,
            ));
            return;
        }
        if self.refs.current.as_deref() != Some(run.branch.as_str()) {
            self.status = Some((
                format!("{} is no longer checked out: rehearse again", run.branch),
                true,
            ));
            return;
        }
        run.applying = true;
        let id = run.id;
        let (branch, other, remote) = (run.branch.clone(), run.other.clone(), run.remote);
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::unmoved(&path, &rehearsal),
            move |tab, checked, ctx| {
                let Some(run) = tab.rehearsing.run.as_mut().filter(|r| r.id == id) else {
                    return;
                };
                run.applying = false;
                if let Err(e) = checked {
                    tab.status = Some((e, true));
                    return;
                }
                let other_ref = BranchRef {
                    name: &other,
                    remote,
                };
                let action = match kind {
                    RehearseKind::Rebase => rebase_action(other_ref, &branch),
                    RehearseKind::Merge => merge_action(other_ref, &branch),
                };
                tab.rehearsing.run = None;
                tab.apply(action, ctx);
            },
        );
    }
}
