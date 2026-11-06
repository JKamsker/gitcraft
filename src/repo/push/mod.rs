//! Push and Sync, and what happens when a push is refused: the remote is checked in the
//! background, and a banner says how the branch and the remote branch stand, with
//! _Pull, then push_ and _Force push (with lease)…_ when that can help. A refusal by the remote
//! itself (a hook, a protected branch) shows git's message in full and offers no force push.

mod banner;
mod check;
mod porcelain;
#[cfg(test)]
mod tests;

use super::messages::{Msg, spawn_or};
use super::operations::expect_ref_at;
use super::pull::PullMode;
use super::{Dialog, OpKind, Refresh, RepoTab, Undo};
use crate::git;
use crate::settings::Settings;

use check::{Checked, Rejection, Target};
use porcelain::Refusal;

/// The labels of Sync's two steps (the pull's is also its undo's).
const SYNC_PULL: &str = "Sync: pull";
const SYNC_PUSH: &str = "Sync: push";

/// What the banner's buttons ask for.
#[derive(Clone, Debug)]
pub(crate) enum PushAct {
    /// Pull from the remote branch the push was refused by (the way `mode` says, else the
    /// settings'), then push again.
    PullThenPush(Option<PullMode>),
    /// Ask before replacing the remote branch with the branch (with a lease).
    ConfirmForce,
    /// Publish the branch on this remote, and track it.
    Publish(String),
    /// Push again to the same place (the remote has nothing the branch doesn't any more).
    PushAgain,
    /// Look at the remote again.
    CheckAgain,
    Dismiss,
}

/// What the banner under the toolbar shows.
#[derive(Clone, Debug)]
pub(crate) enum Banner {
    /// The remote has commits the branch doesn't.
    Rejected(Box<Rejection>),
    /// The remote said no itself: its whole message.
    Refused { branch: String, message: String },
    /// The branch tracks nothing, and there is more than one remote to publish it on.
    NoUpstream {
        branch: String,
        remotes: Vec<String>,
    },
    /// The remote branch the push went to is gone.
    Gone { branch: String, target: Target },
    /// HEAD is on no branch.
    Detached,
}

impl Banner {
    /// The branch it is about, if any: it goes once another one is checked out.
    fn branch(&self) -> Option<&str> {
        match self {
            Banner::Rejected(r) => Some(&r.branch),
            Banner::Refused { branch, .. }
            | Banner::NoUpstream { branch, .. }
            | Banner::Gone { branch, .. } => Some(branch),
            Banner::Detached => None,
        }
    }
}

/// A push started, kept to make sense of its failure.
#[derive(Clone, Debug)]
struct Attempt {
    label: String,
    branch: String,
    /// Where it goes, when it says so itself (a publish, a force push, a push after a pull
    /// from the banner); a plain `git push` goes where git's config says.
    target: Option<Target>,
    /// The commit pushed, for a push of one (a force push).
    commit: Option<String>,
    publish: bool,
}

/// The tab's push state.
#[derive(Default)]
pub(crate) struct PushFlow {
    attempt: Option<Attempt>,
    pub(crate) banner: Option<Banner>,
    /// Sync's push waits for the pull so labelled; to where the banner's pull came from, if
    /// it came from there.
    then_push: Option<(String, Option<Attempt>)>,
}

impl RepoTab {
    /// The toolbar's Push: the checked-out branch, to where it goes.
    pub(super) fn push(&mut self, ctx: &egui::Context) {
        self.push_labelled("Push", ctx);
    }

    fn push_labelled(&mut self, label: &str, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(current) = self.refs.current.clone() else {
            self.push_flow.banner = Some(Banner::Detached);
            return;
        };
        let tracked = self
            .refs
            .local
            .iter()
            .any(|b| b.name == current && !b.upstream.is_empty() && !b.upstream_gone());
        if tracked {
            self.start_push(
                Attempt {
                    label: label.to_owned(),
                    branch: current,
                    target: None,
                    commit: None,
                    publish: false,
                },
                super::ops(&[&["push", "--porcelain"]]),
                ctx,
            );
            return;
        }
        // A first push: publish it and track the remote one, on the one remote there is, or the
        // one picked.
        let remotes: Vec<String> = self.remotes.iter().map(|r| r.name.clone()).collect();
        match remotes.len() {
            0 => self.status = Some(("This repository has no remote to push to".to_owned(), true)),
            1 => self.publish(&current, &remotes[0], label, ctx),
            _ => {
                self.push_flow.banner = Some(Banner::NoUpstream {
                    branch: current,
                    remotes,
                })
            }
        }
    }

    /// Publishes `branch` on `remote` under its own name, and tracks it.
    fn publish(&mut self, branch: &str, remote: &str, label: &str, ctx: &egui::Context) {
        let target = Target {
            remote: remote.to_owned(),
            branch: branch.to_owned(),
        };
        let attempt = Attempt {
            label: label.to_owned(),
            branch: branch.to_owned(),
            target: Some(target),
            commit: None,
            publish: true,
        };
        let steps = vec![publish_args(branch, remote)];
        self.start_push(attempt, steps, ctx);
    }

    fn start_push(&mut self, attempt: Attempt, steps: Vec<Vec<String>>, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let label = attempt.label.clone();
        self.push_flow.banner = None;
        self.push_flow.attempt = Some(attempt);
        self.run_ops(&label, steps, Refresh::All, OpKind::plain(), ctx);
    }

    /// Sync: pulls the checked-out branch the way the settings say, then pushes it. It stops
    /// at the pull when that fails (on conflicts, which are left to resolve).
    pub(super) fn sync(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(current) = self.refs.current.clone() else {
            self.push_flow.banner = Some(Banner::Detached);
            return;
        };
        let tracked = self
            .refs
            .local
            .iter()
            .any(|b| b.name == current && !b.upstream.is_empty() && !b.upstream_gone());
        if !tracked {
            // Nothing to pull from: publishing is all there is to do.
            self.push_labelled(SYNC_PUSH, ctx);
            return;
        }
        let mode = Settings::get(ctx).pull_mode;
        self.sync_pull(super::ops(&[mode.args()]), None, ctx);
    }

    fn sync_pull(&mut self, steps: Vec<Vec<String>>, push: Option<Attempt>, ctx: &egui::Context) {
        self.push_flow.banner = None;
        self.push_flow.then_push = Some((SYNC_PULL.to_owned(), push));
        self.run_undoable(
            SYNC_PULL,
            steps,
            Refresh::All,
            Undo::HeadMove { mode: "--keep" },
            ctx,
        );
    }

    /// What the banner asked for.
    pub(super) fn push_act(&mut self, act: PushAct, ctx: &egui::Context) {
        if self.busy_op.is_some() && !matches!(act, PushAct::Dismiss) {
            return;
        }
        match act {
            PushAct::Dismiss => self.push_flow.banner = None,
            PushAct::Publish(remote) => {
                if let Some(Banner::NoUpstream { branch, .. }) = self.push_flow.banner.take() {
                    self.publish(&branch, &remote, &format!("Push {branch} to {remote}"), ctx);
                }
            }
            PushAct::PushAgain => match self.push_flow.banner.take() {
                Some(Banner::Gone { branch, target }) => {
                    self.push_to(&branch, &target, true, "Push", ctx)
                }
                Some(Banner::Rejected(r)) => {
                    self.push_to(&r.branch, &r.target, r.publish, "Push", ctx)
                }
                other => self.push_flow.banner = other,
            },
            PushAct::PullThenPush(mode) => {
                if let Some(Banner::Rejected(r)) = self.push_flow.banner.clone() {
                    let mode = mode.unwrap_or(Settings::get(ctx).pull_mode);
                    let push = explicit_push(&r.branch, &r.target, r.publish, SYNC_PUSH);
                    self.sync_pull(vec![pull_from(mode, &r.target)], Some(push.0), ctx);
                }
            }
            PushAct::ConfirmForce => {
                if let Some(Banner::Rejected(r)) = &self.push_flow.banner {
                    let (attempt, dialog) = force_dialog(r);
                    self.push_flow.attempt = Some(attempt);
                    self.dialog = Some(dialog);
                }
            }
            PushAct::CheckAgain => {
                if let Some(Banner::Rejected(r)) = self.push_flow.banner.clone() {
                    let q = check::Question {
                        branch: r.branch.clone(),
                        target: Some(r.target.clone()),
                        to: format!("refs/heads/{}", r.target.branch),
                        remotes: self.remote_names(),
                        publish: r.publish,
                        moved: false,
                    };
                    self.check_remote(q, ctx);
                }
            }
        }
    }

    /// Pushes `branch` to `target` explicitly.
    fn push_to(
        &mut self,
        branch: &str,
        target: &Target,
        publish: bool,
        label: &str,
        ctx: &egui::Context,
    ) {
        let (attempt, steps) = explicit_push(branch, target, publish, label);
        self.start_push(attempt, steps, ctx);
    }

    fn remote_names(&self) -> Vec<String> {
        self.remotes.iter().map(|r| r.name.clone()).collect()
    }

    /// Takes in the end of an operation for Push and Sync (called last by `op_done`): Sync's
    /// push after its pull, and a look at the remote after a refused push.
    pub(super) fn push_op_ended(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        if let Some((pull, push)) = self.push_flow.then_push.take() {
            if pull == label {
                self.sync_pulled(push, result, ctx);
                return;
            }
            self.push_flow.then_push = Some((pull, push));
        }
        let Some(attempt) = self.push_flow.attempt.take_if(|a| a.label == label) else {
            return;
        };
        match result {
            Ok(_) => {
                self.push_flow.banner = None;
                if label == SYNC_PUSH {
                    self.status =
                        Some((format!("Sync: {} pulled and pushed", attempt.branch), false));
                }
            }
            // Run again once signed in, and then taken in here again.
            Err(e) if git::is_auth_error(e) => self.push_flow.attempt = Some(attempt),
            Err(e) => self.push_failed(attempt, e, ctx),
        }
    }

    fn sync_pulled(
        &mut self,
        push: Option<Attempt>,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        match result {
            Ok(_) => match push {
                Some(attempt) => {
                    let target = attempt.target.clone();
                    match target {
                        Some(t) => {
                            self.push_to(&attempt.branch, &t, attempt.publish, SYNC_PUSH, ctx)
                        }
                        None => self.push_labelled(SYNC_PUSH, ctx),
                    }
                }
                None => self.push_labelled(SYNC_PUSH, ctx),
            },
            // Pulled again once signed in.
            Err(e) if git::is_auth_error(e) => {
                self.push_flow.then_push = Some((SYNC_PULL.to_owned(), push))
            }
            Err(e) => {
                self.status = Some((
                    format!("Sync stopped at the pull, nothing was pushed: {e}"),
                    true,
                ));
            }
        }
    }

    /// A push that failed: refused by the remote (a banner then says why and what can be done),
    /// or failed otherwise (the status line already says how).
    fn push_failed(&mut self, attempt: Attempt, error: &str, ctx: &egui::Context) {
        let branch_ref = format!("refs/heads/{}", attempt.branch);
        let Some((line, why)) = porcelain::refused(error, &branch_ref, attempt.commit.as_deref())
        else {
            return;
        };
        match why {
            Refusal::Remote(_) => {
                self.push_flow.banner = Some(Banner::Refused {
                    branch: attempt.branch,
                    message: error.trim().to_owned(),
                });
            }
            Refusal::Behind | Refusal::Moved => {
                let q = check::Question {
                    branch: attempt.branch,
                    target: attempt.target,
                    to: line.to,
                    remotes: self.remote_names(),
                    publish: attempt.publish,
                    moved: why == Refusal::Moved,
                };
                self.check_remote(q, ctx);
            }
            Refusal::Other(_) => {}
        }
    }

    /// Looks at the remote in the background (the tab busy meanwhile), then shows the banner.
    fn check_remote(&mut self, q: check::Question, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let what = q
            .target
            .as_ref()
            .map_or_else(|| "the remote".to_owned(), Target::label);
        self.busy_op = Some(format!("Checking {what}"));
        let path = self.path.clone();
        let creds = self.credentials.clone();
        // Let go of all the same if it panics.
        let failed = |e: String| -> Msg {
            Msg::Then(Box::new(move |tab: &mut RepoTab, _: &egui::Context| {
                tab.busy_op = None;
                tab.status = Some((e, true));
            }))
        };
        spawn_or(&self.tx, ctx, failed, move || {
            let checked = check::check(&path, &q, creds.as_ref());
            Msg::Then(Box::new(move |tab: &mut RepoTab, ctx: &egui::Context| {
                tab.remote_checked(checked, ctx)
            }))
        });
    }

    fn remote_checked(&mut self, checked: Result<Checked, String>, ctx: &egui::Context) {
        self.busy_op = None;
        match checked {
            Ok(Checked::Rejected(r)) => {
                self.status = Some((banner::headline(&r), true));
                self.push_flow.banner = Some(Banner::Rejected(r));
            }
            Ok(Checked::Gone { branch, target }) => {
                self.status = Some((format!("{} is gone", target.label()), true));
                self.push_flow.banner = Some(Banner::Gone { branch, target });
            }
            Err(e) => {
                self.status = Some((
                    format!("Push refused, and the remote could not be checked: {e}"),
                    true,
                ))
            }
        }
        // The remote branch was fetched.
        self.reload(ctx);
    }
}

/// A first push of `branch` to `remote`, tracking it: the full ref (a tag of the same name must
/// not be what is pushed), no `:dst` (which would bypass `remote.<name>.push`, Gerrit's
/// refs/for/*), and `simple` (with push.default=upstream, a source alone goes to the branch it
/// tracks).
pub(super) fn publish_args(branch: &str, remote: &str) -> Vec<String> {
    [
        "-c",
        "push.default=simple",
        "push",
        "--porcelain",
        "-u",
        remote,
        &format!("refs/heads/{branch}"),
    ]
    .map(str::to_owned)
    .to_vec()
}

/// A push of `branch` to `target`, named in full; `-u` for a first one.
fn explicit_push(
    branch: &str,
    target: &Target,
    publish: bool,
    label: &str,
) -> (Attempt, Vec<Vec<String>>) {
    let mut args = vec!["push".to_owned(), "--porcelain".to_owned()];
    if publish {
        args.push("-u".to_owned());
    }
    args.push(target.remote.clone());
    args.push(format!("refs/heads/{branch}:refs/heads/{}", target.branch));
    let attempt = Attempt {
        label: label.to_owned(),
        branch: branch.to_owned(),
        target: Some(target.clone()),
        commit: None,
        publish,
    };
    (attempt, vec![args])
}

/// A pull of the remote branch `target` (which need not be the upstream), the way `mode` pulls.
pub(super) fn pull_from(mode: PullMode, target: &Target) -> Vec<String> {
    mode.args()
        .iter()
        .map(|a| (*a).to_owned())
        .chain([
            target.remote.clone(),
            format!("refs/heads/{}", target.branch),
        ])
        .collect()
}

/// A force push of what was checked: the branch's commit then (refused if the branch has
/// moved since), over the remote's commit then (refused by git if the remote has moved since:
/// the lease). `--force-if-includes` where git has it; with the lease's commit given, as here,
/// git does no more with it, the lease is what guards.
pub(super) fn force_push_ops(r: &Rejection) -> Vec<Vec<String>> {
    let dst = format!("refs/heads/{}", r.target.branch);
    let mut push = vec![
        "push".to_owned(),
        "--porcelain".to_owned(),
        format!("--force-with-lease={dst}:{}", r.remote),
    ];
    if r.if_includes {
        push.push("--force-if-includes".to_owned());
    }
    push.push(r.target.remote.clone());
    push.push(format!("{}:{dst}", r.local));
    let mut steps = vec![
        expect_ref_at(&format!("refs/heads/{}", r.branch), &r.local),
        push,
    ];
    // A first push: tracked as `-u` would have (which sets nothing for a commit pushed).
    if r.publish {
        let config = |key: &str, value: &str| {
            vec![
                "config".to_owned(),
                format!("branch.{}.{key}", r.branch),
                value.to_owned(),
            ]
        };
        steps.push(config("remote", &r.target.remote));
        steps.push(config("merge", &dst));
    }
    steps
}

/// The confirmation of a force push, naming what it takes off the remote, and the push it
/// runs as it will be taken in.
fn force_dialog(r: &Rejection) -> (Attempt, Dialog) {
    let title = format!("Force push {} to {}", r.branch, r.target.label());
    let attempt = Attempt {
        label: title.clone(),
        branch: r.branch.clone(),
        target: Some(r.target.clone()),
        commit: Some(r.local.clone()),
        publish: r.publish,
    };
    let dialog = Dialog::confirm(
        &title,
        banner::force_body(r),
        "Force push",
        force_push_ops(r),
        None,
    );
    (attempt, dialog)
}
