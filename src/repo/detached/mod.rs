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

    /// What is checked out, for people: a commit by its short hash, a tag by its name.
    pub fn label(&self) -> String {
        let target = self.target();
        let hash = target.len() == 40 && target.chars().all(|c| c.is_ascii_hexdigit());
        if hash {
            return super::short(target).to_owned();
        }
        target
            .strip_prefix("refs/tags/")
            .unwrap_or(target)
            .to_owned()
    }

    /// The checkout as the views ask for it.
    fn action(self) -> Action {
        match self {
            Leave::Checkout(t) => Action::Checkout(t),
            Leave::CheckoutPull(t) => Action::CheckoutPull(t),
            Leave::CheckoutRemote(t) => Action::CheckoutRemote(t),
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
    /// What the dialog says it checks out.
    pub fn target(&self) -> String {
        self.leave.label()
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
    /// Read again only once HEAD moved: what it was on before doesn't change while it stays,
    /// and while a rebase or the like goes on the banner doesn't show anyway.
    pub(super) fn refresh_detached(&mut self, ctx: &egui::Context) {
        if !self.is_detached() {
            self.detached.reads += 1;
            self.detached.info = None;
            return;
        }
        let read_for = self.detached.info.as_ref().map(|(head, _)| head);
        if read_for == Some(&self.refs.head) || self.work.in_progress.is_some() {
            return;
        }
        self.detached.reads += 1;
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
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                format!("Checkout {}: wait for {busy} to finish", leave.label()),
                true,
            )));
            return;
        }
        if self.detached.checking {
            return;
        }
        self.detached.checking = true;
        let path = self.path.clone();
        let target = leave.target().to_owned();
        self.then(
            ctx,
            // HEAD as it is now, read along: a commit made since the reload is among the
            // commits listed, and is the one a kept branch goes on. A panic still answers, so
            // that checkouts aren't held back for good.
            move || {
                let listed = std::panic::catch_unwind(|| {
                    let now = git::head(&path);
                    git::left_behind(&path, &target).map(|commits| (now, commits))
                });
                listed.unwrap_or_else(|_| {
                    Err(crate::i18n::text("repo-workflows-detached-leave-to-c6023785").to_owned())
                })
            },
            move |tab, found, ctx| {
                tab.detached.checking = false;
                // Something started meanwhile (a commit…): what was listed may be out of date,
                // and the checkout wouldn't start anyway.
                if let Some(busy) = &tab.busy_op {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!(
                            "Didn't check out {}: {busy} started meanwhile",
                            leave.label()
                        ),
                        true,
                    )));
                    return;
                }
                match found {
                    Ok((_, commits)) if commits.is_empty() => tab.leave_now(leave, ctx),
                    Ok((head, commits)) => {
                        tab.dialog = Some(Dialog::KeepDetached(Box::new(KeepDialog {
                            name: suggested_name(&head),
                            commits,
                            head,
                            leave,
                            focus: true,
                        })))
                    }
                    Err(e) => {
                        tab.status = Some(crate::repo::Notice::plain((
                            format!(
                                "Didn't check out {}: couldn't list the commits it would \
                                 leave on no branch ({e})",
                                leave.label()
                            ),
                            true,
                        )))
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
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                format!("Wait for {busy} to finish, then check out again"),
                true,
            )));
            return;
        }
        match act {
            DetachedAct::Leave(leave) => self.leave_now(leave, ctx),
            DetachedAct::Keep { name, head, leave } => {
                let (label, steps, undo) = keep_steps(&name, &head);
                self.detached.resume = Some((label.clone(), leave));
                self.run_undoable(&label, steps, Refresh::All, undo, ctx);
            }
        }
    }

    /// An operation ended: once the branch keeping the commits is made, the checkout asked
    /// for runs (on the next frame the view is shown), checked again first: commits made on
    /// the detached HEAD meanwhile are asked about too.
    pub(super) fn detached_op_done(&mut self, label: &str, ok: bool) {
        let Some((_, leave)) = self.detached.resume.take_if(|(l, _)| l == label) else {
            return;
        };
        if ok {
            self.queued.push(leave.action());
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
