//! The receipt of a rewrite (a rebase, a squash, a reorder, an amend): whether the final files
//! are the same, whether the authors were kept, how many messages changed, what became of the
//! signatures, and each old commit followed to the one that replaced it, paired as
//! `git range-diff` pairs them. Kept with the timeline entry that made it.

mod collect;
pub(crate) use collect::*;

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Entry;
use crate::git;

/// More commits than this on either side are not paired (range-diff is quadratic in them).
const MAX_SERIES: usize = 300;
/// Upstream refs looked at at most, the newest: each is an argument (a command line has a
/// limit).
const MAX_REFS: usize = 300;

/// One commit of either version, as the receipt needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Commit {
    pub(crate) hash: String,
    pub(crate) tree: String,
    /// `Name <email>`.
    pub(crate) author: String,
    pub(crate) summary: String,
    /// The whole message: only while the receipt is made (see [`Pair::message_changed`]).
    #[serde(skip)]
    pub(crate) message: String,
    /// It carries a signature (GPG, SSH or X.509), whether or not it would verify.
    pub(crate) signed: bool,
}

/// How an old commit and a new one relate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PairKind {
    /// The same patch and message.
    Same,
    /// Paired, but the patch or the message differs.
    Changed,
    /// Only in the new version.
    Added,
    /// Only in the old one (dropped, or squashed into another).
    Dropped,
}

/// An old commit and its replacement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pair {
    pub(crate) kind: PairKind,
    pub(crate) old: Option<Commit>,
    pub(crate) new: Option<Commit>,
    /// Both sides are there and their messages differ (worked out when made: the messages
    /// themselves aren't kept).
    #[serde(default)]
    pub(crate) message_changed: bool,
}

impl Pair {
    pub(crate) fn message_changed(&self) -> bool {
        self.message_changed
    }

    /// Both sides are there.
    pub(crate) fn paired(&self) -> bool {
        self.old.is_some() && self.new.is_some()
    }

    pub(crate) fn author_changed(&self) -> bool {
        matches!((&self.old, &self.new), (Some(o), Some(n)) if o.author != n.author)
    }

    pub(crate) fn same_tree(&self) -> bool {
        matches!((&self.old, &self.new), (Some(o), Some(n)) if o.tree == n.tree)
    }
}

/// What a rewrite of a branch did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Receipt {
    /// The branch rewritten (`HEAD` when detached).
    pub(crate) branch: String,
    pub(crate) old_tip: String,
    pub(crate) new_tip: String,
    /// The two tips have the same tree: every file is as it was.
    pub(crate) same_files: bool,
    /// In the order of the new version, each dropped commit where it was.
    pub(crate) pairs: Vec<Pair>,
    /// Too many commits to pair: only the files were compared.
    #[serde(default)]
    pub(crate) unpaired: bool,
}

/// What the receipt says, counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) same: usize,
    pub(crate) changed: usize,
    pub(crate) added: usize,
    pub(crate) dropped: usize,
    pub(crate) authors_changed: usize,
    pub(crate) messages_changed: usize,
    /// Old commits that were signed.
    pub(crate) signed_before: usize,
    /// New ones that are.
    pub(crate) signed_after: usize,
}

impl Receipt {
    pub(crate) fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for p in &self.pairs {
            match p.kind {
                PairKind::Same => c.same += 1,
                PairKind::Changed => c.changed += 1,
                PairKind::Added => c.added += 1,
                PairKind::Dropped => c.dropped += 1,
            }
            c.authors_changed += usize::from(p.author_changed());
            c.messages_changed += usize::from(p.message_changed());
            c.signed_before += usize::from(p.old.as_ref().is_some_and(|o| o.signed));
            c.signed_after += usize::from(p.new.as_ref().is_some_and(|n| n.signed));
        }
        c
    }

    /// The receipt in one line: "final files identical, authors kept, 2 messages changed,
    /// signatures replaced".
    pub(crate) fn sentence(&self) -> String {
        let mut parts = vec![if self.same_files {
            crate::i18n::text("repo-workflows-timeline-sentence-31b36481").to_owned()
        } else {
            crate::i18n::text("repo-workflows-timeline-sentence-99e79f74").to_owned()
        }];
        if self.unpaired {
            parts.push(crate::i18n::text("repo-workflows-timeline-sentence-4a61e684").to_owned());
            return capitalised(&parts.join(", "));
        }
        let c = self.counts();
        let paired = c.same + c.changed > 0;
        match c.authors_changed {
            0 if paired => parts
                .push(crate::i18n::text("repo-workflows-timeline-sentence-d30ebcc6").to_owned()),
            0 => {}
            n => parts.push(crate::i18n::message(
                "repo-workflows-timeline-authors-changed",
                &[("count", (n).into())],
            )),
        }
        match c.messages_changed {
            0 if c.same + c.changed > 0 => parts
                .push(crate::i18n::text("repo-workflows-timeline-sentence-360c0cec").to_owned()),
            0 => {}
            n => parts.push(crate::i18n::message(
                "repo-workflows-timeline-messages-changed",
                &[("count", (n).into())],
            )),
        }
        if let Some(signatures) = signatures(&c) {
            parts.push(signatures);
        }
        let old = c.same + c.changed + c.dropped;
        let new = c.same + c.changed + c.added;
        if old != new {
            parts.push(crate::i18n::message(
                "repo-workflows-timeline-sentence-b5ea0271",
                &[("old", (old).into()), ("new", (new).into())],
            ));
        }
        capitalised(&parts.join(", "))
    }
}

/// What became of the signatures, when any commit had or has one.
fn signatures(c: &Counts) -> Option<String> {
    let (before, after) = (c.signed_before, c.signed_after);
    match (before, after) {
        (0, 0) => None,
        (0, n) => Some(crate::i18n::message(
            "repo-workflows-timeline-commits-signed",
            &[("count", (n).into())],
        )),
        (n, 0) => Some(crate::i18n::message(
            "repo-workflows-timeline-signatures-dropped",
            &[("count", (n).into())],
        )),
        (b, a) if a < b => Some(crate::i18n::message(
            "repo-workflows-timeline-signatures-b59d95ba",
            &[("b-a-1", (b - a).into())],
        )),
        (b, a) if a > b => Some(crate::i18n::message(
            "repo-workflows-timeline-signatures-33c87627",
            &[("a-b-1", (a - b).into())],
        )),
        _ => Some(crate::i18n::text("repo-workflows-timeline-signatures-aa14a1ba").to_owned()),
    }
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod fluent_receipt_tests;
