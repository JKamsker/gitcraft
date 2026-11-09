//! The receipt of a rewrite (a rebase, a squash, a reorder, an amend): whether the final files
//! are the same, whether the authors were kept, how many messages changed, what became of the
//! signatures, and each old commit followed to the one that replaced it, paired as
//! `git range-diff` pairs them. Kept with the timeline entry that made it.

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

/// The branch `entry` rewrote, if it did: one that moved to a commit that doesn't have its old
/// one and isn't one of its old commits either (`None` for a commit, a merge, a fast-forward, a
/// reset back). HEAD's branch is looked at first.
pub(crate) fn rewritten(repo: &Path, entry: &Entry) -> Option<(String, String, String)> {
    let mut candidates: Vec<(String, String, String)> = entry
        .changes()
        .into_iter()
        .filter(|c| c.name != "HEAD" && !c.old.is_empty() && !c.new.is_empty())
        .map(|c| (c.name, c.old, c.new))
        .collect();
    // A detached HEAD moving (bisect, checking out commits) rewrites nothing: only branches.
    candidates.sort_by_key(|(name, ..)| *name != entry.after.branch);
    candidates
        .into_iter()
        .find(|(_, old, new)| !is_ancestor(repo, old, new) && !is_ancestor(repo, new, old))
}

fn is_ancestor(repo: &Path, a: &str, b: &str) -> bool {
    git::run(Some(repo), &["merge-base", "--is-ancestor", a, b]).is_ok()
}

/// The receipt of `branch` going from `old` to `new`.
pub(crate) fn receipt(repo: &Path, branch: &str, old: &str, new: &str) -> Result<Receipt, String> {
    let trees = git::run(
        Some(repo),
        &[
            "rev-parse",
            &format!("{old}^{{tree}}"),
            &format!("{new}^{{tree}}"),
        ],
    )?;
    let trees: Vec<&str> = trees.lines().collect();
    let same_files = matches!(trees.as_slice(), [a, b] if a == b);
    let upstream = upstream_refs(repo, old, new, branch)?;
    let old_series = series(repo, old, new, &upstream)?;
    let new_series = series(repo, new, old, &upstream)?;
    let mut receipt = Receipt {
        branch: branch.to_owned(),
        old_tip: old.to_owned(),
        new_tip: new.to_owned(),
        same_files,
        pairs: Vec::new(),
        unpaired: false,
    };
    if old_series.len() > MAX_SERIES || new_series.len() > MAX_SERIES {
        receipt.unpaired = true;
        return Ok(receipt);
    }
    let Some(headers) = range_diff(repo, &old_series, old, &new_series, new)? else {
        receipt.unpaired = true;
        return Ok(receipt);
    };
    let mut commits = Vec::new();
    // In parts: a command line has a limit (Windows: 32k characters).
    let all: Vec<&String> = old_series.iter().chain(&new_series).collect();
    for part in all.chunks(100) {
        commits.extend(describe(repo, part.iter().copied())?);
    }
    let find = |hash: &str| commits.iter().find(|c| c.hash == hash).cloned();
    receipt.pairs = headers
        .into_iter()
        .map(|h| {
            let (mut old, mut new) = (
                h.old.as_deref().and_then(find),
                h.new.as_deref().and_then(find),
            );
            let message_changed =
                matches!((&old, &new), (Some(o), Some(n)) if o.message.trim() != n.message.trim());
            // As kept: without the messages.
            for c in [&mut old, &mut new].into_iter().flatten() {
                c.message.clear();
            }
            Pair {
                kind: h.kind,
                old,
                new,
                message_changed,
            }
        })
        .collect();
    Ok(receipt)
}

/// Refs a rewrite of `branch` from `old` to `new` may have taken commits from (a rebase's
/// upstream): those that have neither version and aren't part of the old one either. A copy of
/// either version (the upstream before a force push, a backup branch) or a ref on one of the
/// old commits (pushed earlier, a tag, a stacked branch) is not one. The newest [`MAX_REFS`].
fn upstream_refs(repo: &Path, old: &str, new: &str, branch: &str) -> Result<Vec<String>, String> {
    let count = format!("--count={MAX_REFS}");
    let listed = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "--sort=-committerdate",
            &count,
            "--no-contains",
            old,
            "--no-contains",
            new,
            "--no-merged",
            old,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let own = format!("refs/heads/{branch}");
    Ok(listed
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, name)| *name != own)
        .map(|(oid, _)| format!("^{oid}"))
        .collect())
}

/// The commits of `tip` that `other` hasn't, nor any of `upstream` (`^oid`s): those the
/// rewrite took or made. Oldest first, merges left out (as range-diff takes them).
fn series(repo: &Path, tip: &str, other: &str, upstream: &[String]) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec![
        "rev-list".to_owned(),
        "--reverse".to_owned(),
        "--topo-order".to_owned(),
        "--no-merges".to_owned(),
        // One more than is paired at most: enough to know there are too many.
        format!("--max-count={}", MAX_SERIES + 1),
        tip.to_owned(),
        format!("^{other}"),
    ];
    args.extend(upstream.iter().cloned());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(git::run(Some(repo), &args)?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// A pair as range-diff prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) kind: PairKind,
    pub(crate) old: Option<String>,
    pub(crate) new: Option<String>,
}

/// Pairs the two series with `git range-diff`; `None` when the ranges it would take hold too
/// many commits (a merge brought in a lot, or the series starts at the root).
fn range_diff(
    repo: &Path,
    old: &[String],
    old_tip: &str,
    new: &[String],
    new_tip: &str,
) -> Result<Option<Vec<Header>>, String> {
    // A range from the parent of the oldest of the series to its tip; an empty one where the
    // version has none of its own.
    let range = |series: &[String], tip: &str| match series.first() {
        Some(first) => format!("{first}^..{tip}"),
        None => format!("{tip}..{tip}"),
    };
    // The oldest commit may be a root one: then from nothing.
    let range_of = |series: &[String], tip: &str| {
        if series.first().is_some_and(|f| {
            git::run(
                Some(repo),
                &["rev-parse", "--verify", "-q", &format!("{f}^")],
            )
            .is_err()
        }) {
            tip.to_owned()
        } else {
            range(series, tip)
        }
    };
    let (old_range, new_range) = (range_of(old, old_tip), range_of(new, new_tip));
    for range in [&old_range, &new_range] {
        let count = git::run(
            Some(repo),
            &["rev-list", "--count", "--no-merges", range, "--"],
        )?;
        if count.trim().parse::<usize>().unwrap_or(usize::MAX) > MAX_SERIES {
            return Ok(None);
        }
    }
    let out = git::run(
        Some(repo),
        &[
            "range-diff",
            "--no-color",
            "--no-notes",
            "--no-patch",
            "--abbrev=40",
            // Pairs a squash with what it was squashed from more often than the default 60.
            "--creation-factor=90",
            &old_range,
            &new_range,
        ],
    )?;
    let headers = parse_range_diff(&out);
    // What range-diff saw beyond the series (a merge's other side) is not this rewrite's.
    Ok(Some(
        headers
            .into_iter()
            .filter(|h| {
                h.old.as_ref().is_none_or(|o| old.contains(o))
                    && h.new.as_ref().is_none_or(|n| new.contains(n))
            })
            .filter(|h| h.old.is_some() || h.new.is_some())
            .collect(),
    ))
}

/// Reads `git range-diff --no-patch --abbrev=40`: `1:  <hash> = 1:  <hash> Summary`, with `!`
/// for a changed pair, `<` for a dropped commit (`-:  ----` on the right), `>` for an added one.
/// Positions are padded to the width of the largest (` 9:` beside `10:`); what follows a pair
/// is indented by four spaces.
pub(crate) fn parse_range_diff(out: &str) -> Vec<Header> {
    out.lines()
        .filter(|l| !l.starts_with("    "))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let old_pos = words.next()?;
            if !old_pos.ends_with(':') {
                return None;
            }
            let old = words.next()?;
            let mark = words.next()?;
            let _new_pos = words.next()?;
            let new = words.next()?;
            let kind = match mark {
                "=" => PairKind::Same,
                "!" => PairKind::Changed,
                "<" => PairKind::Dropped,
                ">" => PairKind::Added,
                _ => return None,
            };
            let hash = |h: &str| (!h.starts_with('-')).then(|| h.to_owned());
            Some(Header {
                kind,
                old: hash(old),
                new: hash(new),
            })
        })
        .collect()
}

/// Each of `hashes` read in one `log`: its tree, author, message and whether it is signed.
fn describe<'a>(
    repo: &Path,
    hashes: impl Iterator<Item = &'a String>,
) -> Result<Vec<Commit>, String> {
    let mut args: Vec<&str> = vec![
        "log",
        "--no-walk=unsorted",
        "--no-show-signature",
        "--format=raw",
    ];
    let hashes: Vec<&String> = hashes.collect();
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    args.extend(hashes.iter().map(|h| h.as_str()));
    args.push("--");
    Ok(parse_raw(&git::run(Some(repo), &args)?))
}

/// Reads `log --format=raw`: a `commit` line, headers (a signature's lines continue with a
/// space), a blank line, the message indented by four spaces.
pub(crate) fn parse_raw(out: &str) -> Vec<Commit> {
    let mut list: Vec<Commit> = Vec::new();
    let mut in_message = false;
    for line in out.lines() {
        if let Some(hash) = line.strip_prefix("commit ") {
            list.push(Commit {
                hash: hash.split_whitespace().next().unwrap_or("").to_owned(),
                ..Commit::default()
            });
            in_message = false;
            continue;
        }
        let Some(c) = list.last_mut() else {
            continue;
        };
        if in_message {
            if let Some(text) = line.strip_prefix("    ") {
                if !c.message.is_empty() {
                    c.message.push('\n');
                }
                c.message.push_str(text);
                if c.summary.is_empty() {
                    c.summary = text.to_owned();
                }
            } else if line.is_empty() {
                c.message.push('\n');
            }
            continue;
        }
        if line.is_empty() {
            in_message = true;
        } else if let Some(tree) = line.strip_prefix("tree ") {
            c.tree = tree.to_owned();
        } else if let Some(author) = line.strip_prefix("author ") {
            // `Name <email> 1700000000 +0100`: the date isn't who.
            c.author = match author.rfind('>') {
                Some(end) => author[..=end].to_owned(),
                None => author.to_owned(),
            };
        } else if line.starts_with("gpgsig") {
            c.signed = true;
        }
    }
    for c in &mut list {
        c.message = c.message.trim_end().to_owned();
    }
    list
}

#[cfg(test)]
mod fluent_receipt_tests {
    use super::*;
    #[test]
    fn fluent_partition_actual_receipt_sentence_localizes_counts_and_keeps_reflog_identity() {
        let receipt = Receipt {
            branch: "Kunde/ü".into(),
            old_tip: "a".repeat(40),
            new_tip: "b".repeat(40),
            same_files: true,
            unpaired: false,
            pairs: vec![
                Pair {
                    kind: PairKind::Same,
                    old: Some(Commit {
                        author: "Raw Author <raw@example.test>".into(),
                        ..Commit::default()
                    }),
                    new: Some(Commit {
                        author: "Raw Author <raw@example.test>".into(),
                        signed: true,
                        ..Commit::default()
                    }),
                    message_changed: false
                };
                1234
            ],
        };
        for (lang, expected) in [
            ("en-US", "1,234 commits now signed"),
            ("de", "1.234 Commits jetzt signiert"),
        ] {
            crate::i18n::set_language(lang);
            let sentence = receipt.sentence();
            assert!(sentence.contains(expected), "{sentence}");
            assert_eq!(
                super::super::observe::describe_reflog("commit (amend): Raw subject"),
                "Amend “Raw subject”"
            );
            assert_eq!(receipt.branch, "Kunde/ü");
            assert_eq!(receipt.counts().signed_after, 1234);
        }
    }
}
