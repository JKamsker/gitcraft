//! Which local branches are merged, stale or have lost their upstream: read in the background
//! after each change to the branches, shown as badges in the sidebar and offered by _Select
//! stale or merged branches…_.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::forge::PullRequest;
use crate::git::{self, Branch};

/// How many branches not merged outright are checked with `git cherry`, newest first: each is
/// a git run of its own (a round trip for a repository over SSH).
const CHERRY_MAX: usize = 100;

const DAY: i64 = 24 * 60 * 60;

/// How a branch's work got into the default branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Merged {
    /// Its tip is in the base (`git branch --merged`), the base's short name given.
    Into(String),
    /// Every commit of it is in the base under another hash (`git cherry`): rebased or
    /// cherry-picked.
    Rebased(String),
    /// A merged pull request proposed it, at its tip or after: squash-merged, most likely.
    Pr(u64),
}

/// Why a branch is offered for cleanup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    Merged(Merged),
    /// The branch it tracks was deleted on the remote.
    UpstreamGone,
    /// No commit for this many days.
    Stale(i64),
}

impl Reason {
    /// What the checklist and the tooltip say.
    pub fn describe(&self) -> String {
        match self {
            Reason::Merged(Merged::Into(base)) => format!("merged into {base}"),
            Reason::Merged(Merged::Rebased(base)) => format!("its commits are in {base} (rebased)"),
            Reason::Merged(Merged::Pr(n)) => format!("#{n} was merged (squashed)"),
            Reason::UpstreamGone => "deleted on the remote".to_owned(),
            Reason::Stale(days) => format!("no commit for {days} days"),
        }
    }

    /// Whether its work is in the default branch: deleting it loses nothing.
    pub fn is_merged(&self) -> bool {
        matches!(self, Reason::Merged(_))
    }
}

/// What a read found: the branches newest first and how the merged ones were merged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Health {
    /// Local branch names, newest last commit first.
    pub by_date: Vec<String>,
    /// Each branch's last commit time (committer, seconds).
    pub dates: HashMap<String, i64>,
    pub merged: HashMap<String, Merged>,
    /// The default branch: never merged into itself, nor stale.
    pub default: String,
}

impl Health {
    /// Why `b` could go: merged (one way), its upstream gone, no commit for `stale_days` days
    /// (0: never stale) at `now`.
    pub fn reasons(&self, b: &Branch, now: i64, stale_days: u32) -> Vec<Reason> {
        let mut reasons = Vec::new();
        if let Some(how) = self.merged.get(&b.name) {
            reasons.push(Reason::Merged(how.clone()));
        }
        if b.upstream_gone() {
            reasons.push(Reason::UpstreamGone);
        }
        if let Some(days) = self.stale_for(&b.name, now, stale_days) {
            reasons.push(Reason::Stale(days));
        }
        reasons
    }

    /// How many days `name` has had no commit, when that is `stale_days` or more.
    fn stale_for(&self, name: &str, now: i64, stale_days: u32) -> Option<i64> {
        if name == self.default {
            return None;
        }
        let time = *self.dates.get(name)?;
        let days = (now - time) / DAY;
        (stale_days > 0 && days >= i64::from(stale_days)).then_some(days)
    }
}

/// The bases branches are checked against: the default branch here and on its remote.
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub(crate) struct Bases {
    /// The default branch's name (`main`): never offered itself.
    pub name: String,
    /// Full refs that exist, with their commit: `refs/heads/main`, `refs/remotes/origin/main`.
    /// The remote one first, as it is the more up to date for `git cherry`.
    pub refs: Vec<(String, String)>,
}

/// Everything a read depends on; a read is made again only when [`key_of`] them changes.
#[derive(Clone, Debug)]
pub(crate) struct Inputs {
    pub branches: Vec<(String, String)>,
    pub bases: Bases,
    /// `(branch, last commit, number)` of the merged pull requests.
    pub merged_prs: Vec<(String, String, u64)>,
}

impl Inputs {
    pub fn new(local: &[Branch], bases: Bases, merged_prs: &[PullRequest]) -> Self {
        Self {
            branches: local
                .iter()
                .map(|b| (b.name.clone(), b.target.clone()))
                .collect(),
            bases,
            merged_prs: merged_prs
                .iter()
                .map(|p| (p.branch.clone(), p.sha.clone(), p.number))
                .collect(),
        }
    }
}

/// What a read of these would depend on, as a number: checked after every read of the
/// repository, which mostly changes nothing here, without copying the branches.
pub(crate) fn key_of(local: &[Branch], bases: &Bases, merged_prs: &[PullRequest]) -> u64 {
    let mut h = DefaultHasher::new();
    local.len().hash(&mut h);
    for b in local {
        (&b.name, &b.target).hash(&mut h);
    }
    bases.hash(&mut h);
    merged_prs.len().hash(&mut h);
    for p in merged_prs {
        (&p.branch, &p.sha, p.number).hash(&mut h);
    }
    h.finish()
}

/// Reads, with git, what [`Health`] holds. Runs in the background.
pub(crate) fn read(repo: &Path, inputs: &Inputs) -> Result<Health, String> {
    let dated = git::branches_by_date(repo)?;
    let mut merged: HashMap<String, Merged> = HashMap::new();
    for (full, _) in &inputs.bases.refs {
        let short = short_ref(full);
        for name in git::merged_into(repo, full)? {
            merged
                .entry(name)
                .or_insert_with(|| Merged::Into(short.clone()));
        }
    }
    merged.remove(&inputs.bases.name);
    let tips: HashMap<&str, &str> = inputs
        .branches
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let open: Vec<&str> = dated
        .iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| *n != inputs.bases.name && !merged.contains_key(*n))
        .collect();
    let found = rebased_or_squashed(repo, inputs, &open, &tips);
    merged.extend(found);
    Ok(Health {
        by_date: dated.iter().map(|(n, _)| n.clone()).collect(),
        dates: dated.into_iter().collect(),
        merged,
        default: inputs.bases.name.clone(),
    })
}

/// Of the branches `open` (not merged outright), those a merged pull request proposed (at
/// their tip or after) and those `git cherry` finds all in the base.
fn rebased_or_squashed(
    repo: &Path,
    inputs: &Inputs,
    open: &[&str],
    tips: &HashMap<&str, &str>,
) -> HashMap<String, Merged> {
    let mut found = HashMap::new();
    let mut checked: HashSet<&str> = HashSet::new();
    for &name in open {
        let Some(tip) = tips.get(name) else { continue };
        let pr = inputs
            .merged_prs
            .iter()
            .find(|(branch, sha, _)| branch == name && git::is_ancestor(repo, tip, sha));
        if let Some((_, _, number)) = pr {
            found.insert(name.to_owned(), Merged::Pr(*number));
            checked.insert(name);
        }
    }
    let Some((base, _)) = inputs.bases.refs.first() else {
        return found;
    };
    let short = short_ref(base);
    let unchecked = open.iter().filter(|n| !checked.contains(*n));
    for &name in unchecked.take(CHERRY_MAX) {
        let full = format!("refs/heads/{name}");
        if git::cherry_equivalent(repo, base, &full).unwrap_or(false) {
            found.insert(name.to_owned(), Merged::Rebased(short.clone()));
        }
    }
    found
}

/// `main` for `refs/heads/main`, `origin/main` for `refs/remotes/origin/main`.
fn short_ref(full: &str) -> String {
    full.strip_prefix("refs/heads/")
        .or_else(|| full.strip_prefix("refs/remotes/"))
        .unwrap_or(full)
        .to_owned()
}

/// A read asked for, by the key of what it read from.
#[derive(Default)]
pub(crate) struct HealthState {
    /// What the last read that came back found.
    pub health: Option<Health>,
    /// The key of the latest read asked for: an older one's answer is dropped.
    asked: Option<u64>,
    /// The merged pull requests, and for which repository on the forge they were listed.
    pub merged_prs: Vec<PullRequest>,
    pub error: Option<String>,
}

impl HealthState {
    /// Whether to read for `key`: not when that was already asked for. Asks for it.
    pub fn should_read(&mut self, key: u64) -> bool {
        let new = self.asked != Some(key);
        self.asked = Some(key);
        new
    }

    /// Takes in a read's answer, if it is for the latest read asked for.
    pub fn arrived(&mut self, key: u64, result: Result<Health, String>) {
        if self.asked != Some(key) {
            return;
        }
        match result {
            Ok(health) => {
                self.health = Some(health);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Could not tell which branches are merged: {e}")),
        }
    }

    /// Forgets the read asked for, so the next check reads again (the merged pull requests or
    /// the repository changed).
    pub fn invalidate(&mut self) {
        self.asked = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, track: &str) -> Branch {
        Branch {
            name: name.to_owned(),
            target: "a".repeat(40),
            is_head: false,
            upstream: String::new(),
            track: track.to_owned(),
        }
    }

    #[test]
    fn reasons_combine_merged_gone_and_stale() {
        let now = 1_000 * DAY;
        let health = Health {
            by_date: vec!["fresh".to_owned(), "old".to_owned()],
            dates: [
                ("fresh".to_owned(), now - DAY),
                ("old".to_owned(), now - 120 * DAY),
            ]
            .into_iter()
            .collect(),
            merged: [("old".to_owned(), Merged::Pr(7))].into_iter().collect(),
            default: "main".to_owned(),
        };
        assert!(health.reasons(&branch("fresh", ""), now, 90).is_empty());
        assert_eq!(
            health.reasons(&branch("old", "[gone]"), now, 90),
            vec![
                Reason::Merged(Merged::Pr(7)),
                Reason::UpstreamGone,
                Reason::Stale(120)
            ]
        );
        // 0 days: nothing is ever stale.
        assert_eq!(
            health.reasons(&branch("old", ""), now, 0),
            vec![Reason::Merged(Merged::Pr(7))]
        );
        // A branch the read didn't see (made since) has no reasons yet.
        assert!(health.reasons(&branch("new", ""), now, 1).is_empty());
        // The default branch is never stale, however long since its last commit.
        let mut health = health;
        health.dates.insert("main".to_owned(), 0);
        assert!(health.reasons(&branch("main", ""), now, 90).is_empty());
    }

    #[test]
    fn the_key_follows_what_was_read_from() {
        let bases = Bases {
            name: "main".to_owned(),
            refs: vec![("refs/heads/main".to_owned(), "1".repeat(40))],
        };
        let a = key_of(&[branch("x", "")], &bases, &[]);
        let b = key_of(&[branch("y", "")], &bases, &[]);
        assert_eq!(a, key_of(&[branch("x", "")], &bases, &[]));
        assert_ne!(a, b);
        let pr = PullRequest {
            number: 3,
            title: String::new(),
            branch: "x".to_owned(),
            sha: "s".to_owned(),
            author: String::new(),
            url: String::new(),
            draft: false,
        };
        assert_ne!(a, key_of(&[branch("x", "")], &bases, &[pr]));
        let mut state = HealthState::default();
        assert!(state.should_read(a));
        assert!(!state.should_read(a), "the same read is asked for once");
        // An answer for a read since replaced is dropped.
        assert!(state.should_read(b));
        state.arrived(a, Ok(Health::default()));
        assert!(state.health.is_none());
        state.arrived(b, Err("boom".to_owned()));
        assert!(state.error.as_deref().is_some_and(|e| e.contains("boom")));
        state.invalidate();
        assert!(state.should_read(b));
    }

    #[test]
    fn short_names_of_bases() {
        assert_eq!(short_ref("refs/heads/main"), "main");
        assert_eq!(short_ref("refs/remotes/my/fork/main"), "my/fork/main");
        assert_eq!(short_ref("HEAD"), "HEAD");
    }
}
