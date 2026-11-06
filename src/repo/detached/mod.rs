//! A detached HEAD in plain words: a banner saying new commits there belong to no branch, with
//! _Create branch here_ and _Back to <branch>_; and checking out something else after
//! committing there stops first, offering to keep those commits on a branch.

mod banner;
#[cfg(test)]
mod tests;

use super::operations::expect_ref_at;
use super::{Action, Dialog, Refresh, RepoTab, Undo, ops};
use crate::git::{self, DetachedInfo, LooseCommit};

/// What the tab knows about its detached HEAD.
#[derive(Default)]
pub(super) struct Detached {
    /// Read for the HEAD at this commit.
    info: Option<(String, DetachedInfo)>,
    /// Numbers the reads: one that answers after a later one is dropped.
    reads: u64,
    /// A checkout is waiting for the commits it would leave behind to be listed.
    checking: bool,
    /// The checkout to run once the branch made to keep the commits (the operation so
    /// labelled) is there.
    resume: Option<(String, Leave)>,
}

/// A checkout that leaves the detached HEAD, as it was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Leave {
    Checkout(String),
    CheckoutPull(String),
    CheckoutRemote(String),
}

impl Leave {
    /// What is checked out: a branch, a remote branch or a commit.
    pub fn target(&self) -> &str {
        match self {
            Leave::Checkout(t) | Leave::CheckoutPull(t) | Leave::CheckoutRemote(t) => t,
        }
    }
}

/// What the banner and the dialog ask for.
pub(crate) enum DetachedAct {
    /// Check out, leaving behind whatever is on the detached HEAD.
    Leave(Leave),
    /// Keep the commits of the detached HEAD (at `head`) on the new branch `name`, then check
    /// out.
    Keep {
        name: String,
        head: String,
        leave: Leave,
    },
}

/// The dialog shown when a checkout would leave commits on no branch.
pub(crate) struct KeepDialog {
    pub commits: Vec<LooseCommit>,
    pub head: String,
    pub leave: Leave,
    pub name: String,
    pub focus: bool,
}

impl KeepDialog {
    /// What the dialog's buttons say they check out.
    pub fn target(&self) -> &str {
        self.leave.target()
    }

    /// Whether more commits are behind than were listed.
    pub fn more(&self) -> bool {
        self.commits.len() > git::LOOSE_LISTED
    }
}

impl RepoTab {
    /// HEAD is at a commit, on no branch.
    pub(super) fn is_detached(&self) -> bool {
        self.refs.current.is_none() && !self.refs.head.is_empty()
    }

    /// After a read of the refs: reads in the background what the banner needs, while HEAD is
    /// detached.
    pub(super) fn refresh_detached(&mut self, ctx: &egui::Context) {
        self.detached.reads += 1;
        if !self.is_detached() {
            self.detached.info = None;
            return;
        }
        let (path, head, read) = (
            self.path.clone(),
            self.refs.head.clone(),
            self.detached.reads,
        );
        self.then(
            ctx,
            move || git::detached_info(&path),
            move |tab, info, _| {
                if tab.detached.reads == read && tab.is_detached() {
                    tab.detached.info = Some((head, info));
                }
            },
        );
    }

    /// The branch to go back to: the latest HEAD was on that is still there, else `main` or
    /// `master`.
    fn back_to(&self) -> Option<&str> {
        let (_, info) = self.detached.info.as_ref()?;
        let exists = |name: &str| self.refs.local.iter().any(|b| b.name == name);
        info.previous
            .iter()
            .map(String::as_str)
            .chain(["main", "master"])
            .find(|name| exists(name))
    }

    /// Whether the banner shows: detached, and not by a rebase, a bisect or the like, which
    /// say so themselves.
    fn detached_banner_shows(&self) -> bool {
        self.is_detached()
            && self.work.in_progress.is_none()
            && self
                .detached
                .info
                .as_ref()
                .is_some_and(|(head, info)| *head == self.refs.head && !info.bisecting)
    }

    /// Checks `leave` out; from a detached HEAD, only once the commits it would leave on no
    /// branch are listed (in the background), asking first when there are any.
    pub(super) fn leave_to(&mut self, leave: Leave, ctx: &egui::Context) {
        if !self.is_detached() || leave.target().starts_with('-') {
            return self.leave_now(leave, ctx);
        }
        if self.detached.checking || self.busy_op.is_some() {
            return;
        }
        self.detached.checking = true;
        let (path, head) = (self.path.clone(), self.refs.head.clone());
        let target = leave.target().to_owned();
        self.then(
            ctx,
            move || git::left_behind(&path, &target),
            move |tab, found, ctx| {
                tab.detached.checking = false;
                match found {
                    Ok(commits) if commits.is_empty() => tab.leave_now(leave, ctx),
                    Ok(commits) => {
                        tab.dialog = Some(Dialog::KeepDetached(Box::new(KeepDialog {
                            name: suggested_name(&head),
                            commits,
                            head,
                            leave,
                            focus: true,
                        })))
                    }
                    Err(e) => {
                        tab.status = Some((
                            format!(
                                "Checkout {} stopped: could not list the commits it would \
                                 leave on no branch: {e}",
                                leave.target()
                            ),
                            true,
                        ))
                    }
                }
            },
        );
    }

    /// Checks `leave` out at once.
    fn leave_now(&mut self, leave: Leave, ctx: &egui::Context) {
        match leave {
            Leave::Checkout(name) => self.checkout(&name, ctx),
            Leave::CheckoutPull(name) => self.checkout_pull(&name, ctx),
            Leave::CheckoutRemote(name) => self.checkout_remote(&name, ctx),
        }
    }

    pub(super) fn detached_act(&mut self, act: DetachedAct, ctx: &egui::Context) {
        match act {
            DetachedAct::Leave(leave) => self.leave_now(leave, ctx),
            DetachedAct::Keep { name, head, leave } => {
                let (label, steps, undo) = keep_steps(&name, &head);
                if self.busy_op.is_some() {
                    return;
                }
                self.detached.resume = Some((label.clone(), leave));
                self.run_undoable(&label, steps, Refresh::All, undo, ctx);
            }
        }
    }

    /// An operation ended: once the branch keeping the commits is made, the checkout asked
    /// for runs (on the next frame).
    pub(super) fn detached_op_done(&mut self, label: &str, ok: bool) {
        let Some((_, leave)) = self.detached.resume.take_if(|(l, _)| l == label) else {
            return;
        };
        if ok {
            self.queued
                .push(Action::Detached(DetachedAct::Leave(leave)));
        }
    }
}

/// The branch `name` made at `head`, to keep the commits there: its label, its step, and how
/// it is undone (deleted, while it is still at `head`).
fn keep_steps(name: &str, head: &str) -> (String, Vec<Vec<String>>, Undo) {
    let make = ops(&[&["branch", name, head]]);
    let mut undo = vec![expect_ref_at(&format!("refs/heads/{name}"), head)];
    undo.extend(ops(&[&["branch", "-D", "--", name]]));
    (
        format!("Create branch {name}"),
        make.clone(),
        Undo::Custom { undo, redo: make },
    )
}

/// A name for the branch that keeps the commits: `detached-` and the commit's short hash.
fn suggested_name(head: &str) -> String {
    format!("detached-{}", head.get(..7).unwrap_or(head))
}
