//! A checkout git refused because files here would be overwritten: local changes, untracked
//! files, or ignored ones (checkouts run with `--no-overwrite-ignore`). Instead of only the
//! error, a dialog lists them and offers to check out anyway: carrying them over (see
//! [`carry`]), overwriting the ignored files, stashing the changes first, or discarding them.

mod carry;

use super::{Dialog, OpKind, RepoTab, Undo};

/// The files git named, sorted by what they are here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo) struct Blockers {
    /// Tracked files with local changes.
    pub(in crate::repo) changed: Vec<String>,
    /// Untracked files git status lists.
    pub(in crate::repo) untracked: Vec<String>,
    /// Untracked files that are ignored: only `--no-overwrite-ignore` stopped the checkout.
    pub(in crate::repo) ignored: Vec<String>,
}

/// The checkout that failed, what was in its way, and how to run it again.
#[derive(Clone, Debug)]
pub(crate) struct BlockedCheckout {
    /// Its label ("Checkout develop"), kept for the retry.
    pub(in crate::repo) label: String,
    /// Its steps, one of which is the `git checkout`.
    pub(in crate::repo) ops: Vec<Vec<String>>,
    pub(in crate::repo) undo: Undo,
    pub(in crate::repo) blockers: Blockers,
}

/// A way past what is in the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Choice {
    /// Let the checkout replace the ignored files (what plain `git checkout` does).
    OverwriteIgnored,
    /// Keep the files as they are here: changes merged along, the others written back.
    Carry,
    /// Stash everything (untracked files included) first; it stays in the stash list.
    Stash,
    /// `git checkout -f`: local changes to tracked files are thrown away.
    Force,
}

/// The files of git's refusal to check out, if `err` is one: (local changes, untracked or
/// ignored files), as git listed them under each heading.
pub(in crate::repo) fn parse(err: &str) -> Option<(Vec<String>, Vec<String>)> {
    let (mut changed, mut untracked) = (Vec::new(), Vec::new());
    let mut into: Option<&mut Vec<String>> = None;
    for line in err.lines() {
        if line.contains("would be overwritten by checkout:")
            || line.contains("would be removed by checkout:")
        {
            into = Some(if line.contains("untracked") {
                &mut untracked
            } else {
                &mut changed
            });
        } else if let Some(list) = into.as_deref_mut()
            && let Some(file) = line.strip_prefix('\t')
        {
            list.push(unquote(file.trim_end()));
        } else {
            into = None;
        }
    }
    (!changed.is_empty() || !untracked.is_empty()).then_some((changed, untracked))
}

/// A path git quoted (`core.quotePath`) without its quotes; escapes are left as they are.
fn unquote(path: &str) -> String {
    path.strip_prefix('"')
        .and_then(|p| p.strip_suffix('"'))
        .unwrap_or(path)
        .to_owned()
}

impl Blockers {
    /// Sorts what git listed: an untracked file status doesn't show (itself, or a folder of
    /// it) is an ignored one.
    pub(in crate::repo) fn sort(
        changed: Vec<String>,
        listed: Vec<String>,
        work: &crate::git::WorkStatus,
    ) -> Self {
        let shown: Vec<&str> = work
            .unstaged
            .iter()
            .filter(|f| f.code == '?')
            .map(|f| f.path.as_str())
            .collect();
        let (untracked, ignored) = listed.into_iter().partition(|file| {
            shown
                .iter()
                .any(|s| s == file || (s.ends_with('/') && file.starts_with(s)))
        });
        Self {
            changed,
            untracked,
            ignored,
        }
    }

    /// Whether anything besides ignored files is in the way.
    fn local_work(&self) -> bool {
        !self.changed.is_empty() || !self.untracked.is_empty()
    }

    /// The choices that get the checkout through, the one to suggest first. Carrying would
    /// have to delete untracked files git status shows, so it isn't offered with them in the
    /// way; a stash doesn't take ignored files, so it isn't offered with those.
    pub(in crate::repo) fn choices(&self) -> Vec<Choice> {
        let mut choices = Vec::new();
        if self.untracked.is_empty() {
            choices.push(Choice::Carry);
        }
        if !self.local_work() {
            choices.push(Choice::OverwriteIgnored);
        }
        if self.local_work() && self.ignored.is_empty() {
            choices.push(Choice::Stash);
        }
        if self.local_work() {
            choices.push(Choice::Force);
        }
        choices
    }
}

impl BlockedCheckout {
    /// The checkout's steps again, done the way `choice` says; carrying files over needs them
    /// saved first (see [`RepoTab::carry_checkout`]).
    pub(in crate::repo) fn retry(&self, choice: Choice) -> Vec<Vec<String>> {
        let mut ops = self.ops.clone();
        let overwrite = matches!(choice, Choice::Force | Choice::OverwriteIgnored);
        let flag = match choice {
            Choice::Carry => return self.carry(&carry::Carried::default()),
            Choice::Force => Some("-f"),
            Choice::OverwriteIgnored | Choice::Stash => None,
        };
        if let Some(step) = ops
            .iter_mut()
            .find(|op| op.first().is_some_and(|c| c == "checkout"))
        {
            if overwrite {
                step.retain(|arg| arg != "--no-overwrite-ignore");
            }
            if let Some(flag) = flag {
                step.insert(1, flag.to_owned());
            }
        }
        if choice == Choice::Stash {
            let mut stash: Vec<String> = super::operations::stash_push_args()
                .iter()
                .map(|a| (*a).to_owned())
                .collect();
            stash.extend(["-m".to_owned(), format!("Before {}", self.label)]);
            ops.insert(0, stash);
        }
        ops
    }
}

/// Whether `ops` can run again with its checkout changed: steps that ran before it (a fork's
/// `remote add`) would run a second time, so only checks may come first.
fn retryable(ops: &[Vec<String>]) -> bool {
    carry::checkout_at(ops).is_some_and(|at| {
        ops[..at]
            .iter()
            .all(|step| super::operations::is_expectation(step))
    })
}

impl RepoTab {
    /// Opens the dialog for a checkout git refused because of files in the way; whether it
    /// did.
    pub(in crate::repo) fn offer_blocked_checkout(&mut self, label: &str, err: &str) -> bool {
        let Some((changed, listed)) = parse(err) else {
            return false;
        };
        let Some(op) = self.last_op.as_ref().filter(|op| op.label == label) else {
            return false;
        };
        if !retryable(&op.ops) {
            return false;
        }
        let undo = match &op.kind {
            OpKind::Normal(undo, _) => undo.clone(),
            OpKind::Undo(_) | OpKind::Redo(_) => return false,
        };
        self.dialog = Some(Dialog::CheckoutBlocked(Box::new(BlockedCheckout {
            label: label.to_owned(),
            ops: op.ops.clone(),
            undo,
            blockers: Blockers::sort(changed, listed, &self.work),
        })));
        true
    }
}

#[cfg(test)]
#[path = "checkout_blocked_tests.rs"]
mod tests;
