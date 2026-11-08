//! _Cut release…_ in the tab: its dialog (the version suggested, the notes to edit, what to do,
//! and what each step will do), the steps run one after another in the background with how
//! each went, _Resume_ from the one that failed, and the undo of what was done here.

#[cfg(test)]
mod tests;
mod view;

use std::path::PathBuf;

use egui::Context;

use super::{RepoTab, UndoEntry};
use crate::release::{self, Done, Draft, Plan, Step, notes};

/// The release being prepared or cut.
pub(crate) struct ReleaseView {
    /// The commit released (as asked: a hash).
    commit: String,
    /// `None` while it is read.
    draft: Option<Result<Draft, String>>,
    form: Form,
    /// The steps, once started.
    run: Option<Run>,
}

/// What the dialog lets the user choose.
#[derive(Clone, Debug, Default)]
pub(crate) struct Form {
    pub tag: String,
    pub notes: String,
    pub bump: bool,
    pub changelog: bool,
    pub sign: bool,
    pub push: bool,
    pub publish: bool,
    pub prerelease: bool,
    pub assets: Vec<PathBuf>,
}

/// How a step went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Waiting,
    Running,
    Done,
    /// Left out after it failed.
    Skipped,
    Failed(String),
}

/// The steps of a release started.
pub(crate) struct Run {
    plan: Plan,
    steps: Vec<(Step, Status)>,
    done: Done,
    /// The undo entry recorded for what was done here, by its label.
    recorded: bool,
}

impl Run {
    fn running(&self) -> bool {
        self.steps.iter().any(|(_, s)| *s == Status::Running)
    }

    fn finished(&self) -> bool {
        self.steps
            .iter()
            .all(|(_, s)| matches!(s, Status::Done | Status::Skipped))
    }

    /// The first step not done (nor skipped).
    fn next(&self) -> Option<usize> {
        self.steps
            .iter()
            .position(|(_, s)| !matches!(s, Status::Done | Status::Skipped))
    }
}

/// What the release dialog asks for.
pub(crate) enum ReleaseAct {
    /// Prepare a release of the commit.
    Open(String),
    Close,
    /// Start the steps, or go on from the one that failed.
    Run,
    AddAssets,
    RemoveAsset(usize),
    /// Mark the step that failed done, to go on without it.
    Skip,
}

impl RepoTab {
    pub(super) fn release_act(&mut self, act: ReleaseAct, ctx: &Context) {
        match act {
            ReleaseAct::Open(commit) => self.open_release(commit, ctx),
            ReleaseAct::Close => {
                // Not while a step runs: it would come back to nothing.
                if !self
                    .release
                    .as_ref()
                    .and_then(|r| r.run.as_ref())
                    .is_some_and(Run::running)
                {
                    self.release = None;
                }
            }
            ReleaseAct::Run => self.run_release(ctx),
            ReleaseAct::AddAssets => {
                let Some(view) = &mut self.release else {
                    return;
                };
                if let Some(files) = rfd::FileDialog::new()
                    .set_title(crate::i18n::text(
                        "repo-workflows-release-release-act-2aead8f8",
                    ))
                    .pick_files()
                {
                    for f in files {
                        if !view.form.assets.contains(&f) {
                            view.form.assets.push(f);
                        }
                    }
                }
            }
            ReleaseAct::Skip => {
                if let Some(run) = self.release.as_mut().and_then(|r| r.run.as_mut())
                    && !run.running()
                    && let Some(step) = run
                        .steps
                        .iter_mut()
                        .find(|(_, s)| matches!(s, Status::Failed(_)))
                {
                    step.1 = Status::Skipped;
                }
            }
            ReleaseAct::RemoveAsset(i) => {
                if let Some(view) = &mut self.release
                    && i < view.form.assets.len()
                {
                    view.form.assets.remove(i);
                }
            }
        }
    }

    fn open_release(&mut self, commit: String, ctx: &Context) {
        if self
            .release
            .as_ref()
            .and_then(|r| r.run.as_ref())
            .is_some_and(Run::running)
        {
            return;
        }
        self.release = Some(ReleaseView {
            commit: commit.clone(),
            draft: None,
            form: Form::default(),
            run: None,
        });
        let repo = self.path.clone();
        let remote = self.default_remote();
        let hosting = self.hosting.clone();
        self.then(
            ctx,
            {
                let commit = commit.clone();
                move || release::draft(&repo, &commit, remote)
            },
            move |tab, draft, _| {
                let Some(view) = &mut tab.release else { return };
                if view.commit != commit {
                    return;
                }
                if let Ok(d) = &draft {
                    let link = |n: u64| hosting.as_ref().map(|h| release::pr_url(h, n));
                    view.form = Form {
                        tag: d.suggested.to_string(),
                        notes: notes::notes(&d.changes, &link),
                        bump: !d.manifests.is_empty(),
                        changelog: d.changelog && d.manifest_note.is_none(),
                        sign: d.sign,
                        push: d.remote.is_some(),
                        publish: hosting.is_some() && d.remote.is_some(),
                        prerelease: d.suggested.is_pre(),
                        assets: Vec::new(),
                    };
                }
                view.draft = Some(draft);
            },
        );
    }

    /// Starts the steps, or goes on from the first one not done.
    fn run_release(&mut self, ctx: &Context) {
        let Some(view) = &mut self.release else {
            return;
        };
        if self.busy_op.is_some() && view.run.as_ref().is_none_or(|r| !r.running()) {
            self.status = Some(crate::repo::Notice::plain((
                "Wait for the operation running to end".to_owned(),
                true,
            )));
            return;
        }
        if view.run.is_none() {
            let Some(Ok(draft)) = &view.draft else { return };
            let plan = plan_of(draft, &view.form);
            let steps = plan
                .steps()
                .into_iter()
                .map(|s| (s, Status::Waiting))
                .collect();
            view.run = Some(Run {
                plan,
                steps,
                done: Done::default(),
                recorded: false,
            });
        }
        let label = view
            .run
            .as_ref()
            .map(|r| {
                crate::i18n::message(
                    "repo-workflows-release-run-release-fbe3bbf7",
                    &[("r-plan-tag-1", (r.plan.tag).clone().into())],
                )
            })
            .unwrap_or_default();
        self.busy_op = Some(label);
        self.op_started = Some(std::time::Instant::now());
        // What the steps done left must still be there (an undo may have taken it back).
        let Some(run) = self.release.as_ref().and_then(|v| v.run.as_ref()) else {
            return;
        };
        let (plan, done) = (run.plan.clone(), run.done.clone());
        let repo = self.path.clone();
        self.then(
            ctx,
            move || still_there(&repo, &plan, &done),
            |tab, gone, ctx| match gone {
                None => tab.next_release_step(ctx),
                Some(why) => {
                    tab.busy_op = None;
                    tab.op_started = None;
                    tab.status = Some(crate::repo::Notice::plain((why, true)));
                }
            },
        );
    }

    /// Runs the first step not done, then the next, until one fails or all are done.
    fn next_release_step(&mut self, ctx: &Context) {
        let Some(run) = self.release.as_mut().and_then(|v| v.run.as_mut()) else {
            self.busy_op = None;
            return;
        };
        let Some(i) = run.next() else {
            self.release_stopped(ctx);
            return;
        };
        run.steps[i].1 = Status::Running;
        let step = run.steps[i].0;
        let plan = run.plan.clone();
        let mut done = run.done.clone();
        let repo = self.path.clone();
        let creds = self.credentials.clone();
        let hosting = self.hosting.clone();
        self.then(
            ctx,
            move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    release::run_step(
                        &repo,
                        &plan,
                        step,
                        &mut done,
                        creds.as_ref(),
                        hosting.as_ref(),
                    )
                }))
                .unwrap_or_else(|_| {
                    Err(
                        crate::i18n::text("repo-workflows-release-next-release-step-1dac8dea")
                            .to_owned(),
                    )
                });
                (result, done)
            },
            move |tab, (result, done), ctx| {
                let Some(run) = tab.release.as_mut().and_then(|v| v.run.as_mut()) else {
                    tab.busy_op = None;
                    return;
                };
                run.done = done;
                match result {
                    Ok(()) => {
                        run.steps[i].1 = Status::Done;
                        tab.next_release_step(ctx);
                    }
                    Err(e) => {
                        let auth = crate::git::is_auth_error(&e);
                        run.steps[i].1 = Status::Failed(e);
                        tab.release_stopped(ctx);
                        // Signing in, then Resume, pushes with what was typed.
                        if auth {
                            let url = tab
                                .default_remote()
                                .and_then(|r| tab.remotes.iter().find(|x| x.name == r))
                                .map(|r| r.url.clone())
                                .unwrap_or_default();
                            tab.last_op = None;
                            tab.dialog = Some(super::Dialog::credentials(&url, false));
                        }
                    }
                }
            },
        );
    }

    /// The steps stopped (all done, or one failed): what was done here becomes undoable, the
    /// tab is free again and reads the repository.
    fn release_stopped(&mut self, ctx: &Context) {
        self.busy_op = None;
        let started = self.op_started.take();
        let mut remember = None;
        let Some(run) = self.release.as_mut().and_then(|v| v.run.as_mut()) else {
            return;
        };
        let label = crate::i18n::message(
            "repo-workflows-release-release-stopped-fbe3bbf7",
            &[("run-plan-tag-1", (run.plan.tag).clone().into())],
        );
        let local_done = run.done.bump_commit.is_some() || run.done.tag_object.is_some();
        if local_done {
            let (undo, redo) = undo(&run.plan, &run.done);
            let entry = UndoEntry {
                id: 0,
                agent: None,
                label: label.clone(),
                undo,
                redo,
                heads: None,
            };
            remember = Some((entry, run.recorded));
            run.recorded = true;
        }
        let failed = run.steps.iter().find_map(|(step, s)| match s {
            Status::Failed(e) => Some((*step, e.clone())),
            _ => None,
        });
        self.status = Some(crate::repo::Notice::plain(match failed {
            Some((step, e)) => (format!("{label}: {} failed: {e}", step.label()), true),
            None if run.finished() => (format!("Released {}", run.plan.tag), false),
            None => (format!("{label}: stopped"), false),
        }));
        if let Some((entry, replace)) = remember {
            if replace {
                self.replace_recent_undo(entry, ctx);
            } else {
                self.push_undo(entry, ctx);
            }
        }
        let long = started.is_some_and(|t| t.elapsed() > std::time::Duration::from_secs(4));
        if long {
            self.finished
                .push(crate::repo::Notice::plain(match self.status.as_deref() {
                    Some((text, failed)) => (format!("{} · {text}", self.name), *failed),
                    None => (format!("{} · {label}", self.name), false),
                }));
        }
        self.reload(ctx);
    }
}

/// The release the form asks for.
fn plan_of(draft: &Draft, form: &Form) -> Plan {
    let tag = form.tag.trim().to_owned();
    let version = release::version::Version::parse(&tag)
        .map(|v| v.number())
        .unwrap_or_else(|| {
            tag.trim_start_matches(|c: char| !c.is_ascii_digit())
                .to_owned()
        });
    let bump = (form.bump || form.changelog)
        .then(|| draft.branch.clone())
        .flatten()
        .filter(|_| draft.manifest_note.is_none())
        .map(|branch| release::Bump {
            remote_ref: draft
                .remote_ref
                .clone()
                .unwrap_or_else(|| format!("refs/heads/{branch}")),
            branch,
            version,
            manifests: if form.bump {
                draft.manifests.clone()
            } else {
                Vec::new()
            },
            changelog: form.changelog && draft.changelog,
        });
    Plan {
        commit: draft.commit.clone(),
        tag,
        notes: form.notes.trim().to_owned(),
        bump,
        sign: form.sign,
        push: draft.remote.clone().filter(|_| form.push),
        publish: form.publish && form.push,
        prerelease: form.prerelease,
        assets: if form.publish {
            form.assets.clone()
        } else {
            Vec::new()
        },
    }
}

/// How the local steps done are undone (and redone): the tag deleted, the version commit taken
/// back off the branch, each only while it is still as the release left it. What was pushed and
/// published stays.
pub(crate) fn undo(plan: &Plan, done: &Done) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    use super::operations::{expect_ref_at, expect_symref};
    let s = |args: &[&str]| {
        args.iter()
            .map(|a| (*a).to_owned())
            .collect::<Vec<String>>()
    };
    let tag_ref = format!("refs/tags/{}", plan.tag);
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    // The tag checked by its commit (what the check reads), put back as its object.
    if let Some(commit) = &done.tag_commit {
        undo.push(expect_ref_at(&tag_ref, commit));
    }
    if let (Some(bump), Some(commit)) = (&plan.bump, &done.bump_commit) {
        let branch = format!("refs/heads/{}", bump.branch);
        undo.push(expect_symref("HEAD", &branch));
        undo.push(expect_ref_at(&branch, commit));
        redo.push(expect_symref("HEAD", &branch));
        redo.push(expect_ref_at(&branch, &plan.commit));
        redo.push(s(&["reset", "--keep", commit]));
    }
    if done.tag_object.is_some() {
        undo.push(s(&["tag", "-d", &plan.tag]));
    }
    if done.bump_commit.is_some() {
        undo.push(s(&["reset", "--keep", &plan.commit]));
    }
    if let Some(object) = &done.tag_object {
        redo.push(s(&["update-ref", &tag_ref, object, ""]));
    }
    (undo, redo)
}

/// Why what the steps done left isn't there any more (`None`: it is), before resuming.
fn still_there(repo: &std::path::Path, plan: &Plan, done: &Done) -> Option<String> {
    use crate::git;
    if let Some(commit) = &done.tag_commit
        && git::resolve(repo, &format!("refs/tags/{}", plan.tag))
            .ok()
            .as_ref()
            != Some(commit)
    {
        return Some(crate::i18n::message(
            "repo-workflows-release-still-there-a632e5ba",
            &[("plan-tag-1", (plan.tag).clone().into())],
        ));
    }
    if let (Some(bump), Some(commit)) = (&plan.bump, &done.bump_commit)
        && git::run(
            Some(repo),
            &[
                "merge-base",
                "--is-ancestor",
                commit,
                &format!("refs/heads/{}", bump.branch),
            ],
        )
        .is_err()
    {
        return Some(crate::i18n::message(
            "repo-workflows-release-still-there-46993d84",
            &[("bump-branch-1", (bump.branch).clone().into())],
        ));
    }
    None
}
