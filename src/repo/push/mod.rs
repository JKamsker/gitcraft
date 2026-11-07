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

/// The labels of Sync's steps (the pull's is also its undo's).
const SYNC_PULL: &str = "Sync: pull";
const SYNC_PUSH: &str = "Sync: push";
const SYNC_PUBLISH: &str = "Sync: publish";

/// What the banner's buttons ask for.
#[derive(Clone, Debug)]
pub(crate) enum PushAct {
    /// Pull from the remote branch the push was refused by, the way the settings say: the
    /// branch is only behind, there is nothing to push.
    Pull,
    /// Pull from the remote branch the push was refused by (the way `mode` says, else the
    /// settings'), then push again.
    PullThenPush(Option<PullMode>),
    /// Ask before replacing the remote branch with the branch (with a lease).
    ConfirmForce,
    /// Publish the branch on this remote, and track it.
    Publish(String),
    /// Push again to the same place (the remote has nothing the branch doesn't any more, or
    /// its branch is gone).
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
    Gone {
        branch: String,
        target: Target,
        publish: bool,
    },
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
    /// What the status line says once it went through, if more than "done".
    done: Option<String>,
}

/// A push waiting for an operation to end: Sync's push, after its pull.
#[derive(Clone, Debug)]
struct Chain {
    /// The label of the operation it waits for.
    after: String,
    /// What is pushed then: to the place the banner's pull came from, or (`target: None`)
    /// where git's config says.
    push: Attempt,
    /// The pull is done, and what is waited for now is what ran after it (updating the
    /// submodules): the push follows whatever came of that.
    pulled: bool,
}

/// The tab's push state.
#[derive(Default)]
pub(crate) struct PushFlow {
    attempt: Option<Attempt>,
    pub(crate) banner: Option<Banner>,
    then_push: Option<Chain>,
}

impl RepoTab {
    /// The toolbar's Push: the checked-out branch, to where it goes.
    pub(super) fn push(&mut self, ctx: &egui::Context) {
        if self.echo_before_push(ctx) {
            return;
        }
        if let Some(dialog) = self.review_dialog() {
            self.dialog = Some(dialog);
            return;
        }
        self.push_labelled("Push", None, ctx);
    }

    /// The checked-out branch, as `label`; `done`: the status line once it went through.
    fn push_labelled(&mut self, label: &str, done: Option<String>, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(current) = self.refs.current.clone() else {
            self.push_flow.banner = Some(Banner::Detached);
            return;
        };
        if let Some(mut args) = super::forks::plan::renamed_push(self.forks.setup(), &current) {
            args.insert(1, "--porcelain".to_owned());
            let attempt = Attempt {
                label: label.to_owned(),
                branch: current,
                target: None,
                commit: None,
                publish: false,
                done,
            };
            self.start_push(attempt, vec![args], ctx);
            return;
        }
        if self.tracks(&current) {
            let attempt = Attempt {
                label: label.to_owned(),
                branch: current,
                target: None,
                commit: None,
                publish: false,
                done,
            };
            self.start_push(attempt, super::ops(&[&["push", "--porcelain"]]), ctx);
            return;
        }
        // A first push: publish it and track the remote one, on the one remote there is, or the
        // one picked.
        let remotes = self.remote_names();
        match remotes.len() {
            0 => self.status = Some(("This repository has no remote to push to".to_owned(), true)),
            1 => self.publish(&current, &remotes[0], label, done, ctx),
            _ => {
                self.push_flow.banner = Some(Banner::NoUpstream {
                    branch: current,
                    remotes,
                })
            }
        }
    }

    /// Whether `branch` tracks a remote branch that is still there.
    fn tracks(&self, branch: &str) -> bool {
        self.refs
            .local
            .iter()
            .any(|b| b.name == branch && !b.upstream.is_empty() && !b.upstream_gone())
    }

    /// Publishes `branch` on `remote` and tracks it (under its own name, unless the remote's
    /// push refspecs say otherwise).
    fn publish(
        &mut self,
        branch: &str,
        remote: &str,
        label: &str,
        done: Option<String>,
        ctx: &egui::Context,
    ) {
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
            done,
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
        if let Some(dialog) = self.review_dialog() {
            self.dialog = Some(dialog);
            return;
        }
        if self.busy_op.is_some() {
            return;
        }
        let Some(current) = self.refs.current.clone() else {
            self.push_flow.banner = Some(Banner::Detached);
            return;
        };
        if !self.tracks(&current) {
            // Nothing to pull from: publishing is all there is to do.
            let done = format!("Sync: {current} published (it had nothing to pull)");
            self.push_labelled(SYNC_PUBLISH, Some(done), ctx);
            return;
        }
        let mode = Settings::get(ctx).pull_mode;
        let push = Attempt {
            label: SYNC_PUSH.to_owned(),
            branch: current.clone(),
            target: None,
            commit: None,
            publish: false,
            done: Some(format!("Sync: {current} pulled and pushed")),
        };
        self.pull_then(SYNC_PULL, super::ops(&[mode.args()]), push, ctx);
    }

    /// Pulls (undoably) as `label`, then runs `push`.
    fn pull_then(
        &mut self,
        label: &str,
        steps: Vec<Vec<String>>,
        push: Attempt,
        ctx: &egui::Context,
    ) {
        self.push_flow.banner = None;
        self.push_flow.then_push = Some(Chain {
            after: label.to_owned(),
            push,
            pulled: false,
        });
        self.pull_steps(label, steps, ctx);
    }

    fn pull_steps(&mut self, label: &str, steps: Vec<Vec<String>>, ctx: &egui::Context) {
        self.run_undoable(
            label,
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
                    let label = format!("Push {branch} to {remote}");
                    self.publish(&branch, &remote, &label, None, ctx);
                }
            }
            PushAct::PushAgain => match self.push_flow.banner.take() {
                Some(Banner::Gone {
                    branch,
                    target,
                    publish,
                }) => self.push_to(&branch, &target, publish, ctx),
                Some(Banner::Rejected(r)) => self.push_to(&r.branch, &r.target, r.publish, ctx),
                other => self.push_flow.banner = other,
            },
            PushAct::Pull => {
                if let Some(Banner::Rejected(r)) = self.push_flow.banner.take() {
                    let mode = Settings::get(ctx).pull_mode;
                    let label = format!("Pull {}", r.target.label());
                    self.pull_steps(&label, vec![pull_from(mode, &r.target)], ctx);
                }
            }
            PushAct::PullThenPush(mode) => {
                if let Some(Banner::Rejected(r)) = self.push_flow.banner.clone() {
                    let mode = mode.unwrap_or(Settings::get(ctx).pull_mode);
                    let (mut push, _) = explicit_push(&r.branch, &r.target, r.publish, "Push");
                    push.done = Some(format!(
                        "{} pulled from {} and pushed",
                        r.branch,
                        r.target.label()
                    ));
                    let label = format!("Pull {}", r.target.label());
                    self.pull_then(&label, vec![pull_from(mode, &r.target)], push, ctx);
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
    fn push_to(&mut self, branch: &str, target: &Target, publish: bool, ctx: &egui::Context) {
        let (attempt, steps) = explicit_push(branch, target, publish, "Push");
        self.start_push(attempt, steps, ctx);
    }

    /// Runs the push a chain waited for.
    fn chained_push(&mut self, attempt: Attempt, ctx: &egui::Context) {
        match attempt.target.clone() {
            Some(t) => {
                let (mut explicit, steps) =
                    explicit_push(&attempt.branch, &t, attempt.publish, &attempt.label);
                explicit.done = attempt.done;
                self.start_push(explicit, steps, ctx);
            }
            None => self.push_labelled(&attempt.label, attempt.done, ctx),
        }
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
        if let Some(chain) = self.push_flow.then_push.take() {
            if chain.after == label {
                self.chain_step_ended(chain, result, ctx);
                return;
            }
            self.push_flow.then_push = Some(chain);
        }
        let Some(attempt) = self.push_flow.attempt.take_if(|a| a.label == label) else {
            return;
        };
        let branch_ref = format!("refs/heads/{}", attempt.branch);
        let refused = |out: &str| porcelain::refused(out, &branch_ref, attempt.commit.as_deref());
        match result {
            // Older git ends a push some of whose refs were refused with success all the same.
            Ok(out) if refused(out).is_some() => {
                let out = out.clone();
                self.status = Some((format!("{label} refused: {}", out.trim()), true));
                self.push_failed(attempt, &out, ctx);
            }
            Ok(_) => {
                self.push_flow.banner = None;
                if let Some(done) = attempt.done {
                    self.status = Some((done, false));
                }
            }
            // Run again once signed in, and then taken in here again. Not a refusal whose
            // message (a hook's words) happens to say "403".
            Err(e) if refused(e).is_none() && git::is_auth_error(e) => {
                self.push_flow.attempt = Some(attempt)
            }
            Err(e) => self.push_failed(attempt, e, ctx),
        }
    }

    /// The operation a chain waited for ended.
    fn chain_step_ended(
        &mut self,
        mut chain: Chain,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        // After the pull, or after what ran after it (whatever came of that).
        if chain.pulled || result.is_ok() {
            // Something else started meanwhile (updating the submodules after the pull): the
            // push waits for it.
            if let Some(busy) = self.busy_op.clone() {
                chain.after = busy;
                chain.pulled = true;
                self.push_flow.then_push = Some(chain);
            } else {
                self.chained_push(chain.push, ctx);
            }
            return;
        }
        match result {
            // Pulled again once signed in.
            Err(e) if git::is_auth_error(e) => self.push_flow.then_push = Some(chain),
            Err(e) => {
                let what = chain.after;
                self.status = Some((format!("{what} failed, so nothing was pushed: {e}"), true));
            }
            Ok(_) => {}
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
                let Some(target) = refused_target(attempt.target, &line.to) else {
                    return;
                };
                let q = check::Question {
                    branch: attempt.branch,
                    target,
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
                self.status = Some((format!("Push refused: {}", banner::headline(&r)), true));
                self.push_flow.banner = Some(Banner::Rejected(r));
            }
            Ok(Checked::Gone {
                branch,
                target,
                publish,
            }) => {
                self.status = Some((format!("Push refused: {} is gone", target.label()), true));
                self.push_flow.banner = Some(Banner::Gone {
                    branch,
                    target,
                    publish,
                });
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

/// Where a refused push to `to` (the ref the refused line names) went: only a branch can be
/// looked at and replaced (not Gerrit's `refs/for/main`: `None`). The remote is the one the push
/// named, if it did (`Some(None)` leaves it to git's config); the branch is the one the line
/// names, as a remote's push refspecs may have sent it under another name than the one asked.
pub(super) fn refused_target(asked: Option<Target>, to: &str) -> Option<Option<Target>> {
    let name = to.strip_prefix("refs/heads/")?;
    Some(asked.map(|t| Target {
        remote: t.remote,
        branch: name.to_owned(),
    }))
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
        "--",
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
    args.push("--".to_owned());
    args.push(target.remote.clone());
    args.push(format!("refs/heads/{branch}:refs/heads/{}", target.branch));
    let attempt = Attempt {
        label: label.to_owned(),
        branch: branch.to_owned(),
        target: Some(target.clone()),
        commit: None,
        publish,
        done: None,
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
    push.push("--".to_owned());
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
        done: None,
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
