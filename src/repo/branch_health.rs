//! Which local branches are merged, stale or have lost their upstream: read in the background
//! after each change to the branches, shown as badges in the sidebar and offered by _Select
//! stale or merged branches…_.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::forge::MergedPullRequest;
use crate::git::{self, Branch};

/// How many branches not merged outright are checked with `git cherry`, newest first: each is
/// a git run of its own (a round trip for a repository over SSH). Answers are kept by branch
/// and base commit, so a later read only checks the branches that moved.
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
    /// A pull request into the default branch (named) proposed it, at its tip or after, and
    /// was merged: squash-merged, most likely.
    Pr(u64, String),
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
            Reason::Merged(Merged::Into(base)) => {
                crate::i18n::message("repo-core-0241", &[("base", base.to_string().into())])
            }
            Reason::Merged(Merged::Rebased(base)) => {
                crate::i18n::message("repo-core-0242", &[("base", base.to_string().into())])
            }
            Reason::Merged(Merged::Pr(n, base)) => crate::i18n::message(
                "repo-core-0243",
                &[("n", (*n).into()), ("base", base.to_string().into())],
            ),
            Reason::UpstreamGone => crate::i18n::text("repo-core-0244").to_owned(),
            Reason::Stale(days) => {
                crate::i18n::message("repo-core-0245", &[("days", (*days).into())])
            }
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
    /// How each merged branch was merged, and the commit it was at when that was found: a
    /// branch that got commits since is not known to be merged any more.
    pub merged: HashMap<String, (String, Merged)>,
    /// The default branch: never merged into itself, nor stale.
    pub default: String,
}

impl Health {
    /// Why `b` could go: merged (one way), its upstream gone, no commit for `stale_days` days
    /// (0: never stale) at `now`.
    pub fn reasons(&self, b: &Branch, now: i64, stale_days: u32) -> Vec<Reason> {
        let mut reasons = Vec::new();
        let merged = self.merged.get(&b.name).filter(|(at, _)| *at == b.target);
        if let Some((_, how)) = merged {
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

    /// Forgets what the merged pull requests said: they were another repository's.
    pub fn forget_prs(&mut self) {
        self.merged
            .retain(|_, (_, how)| !matches!(how, Merged::Pr(..)));
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
    /// The merged pull requests into the default branch.
    pub merged_prs: Vec<MergedPullRequest>,
}

impl Inputs {
    pub fn new(local: &[Branch], bases: Bases, merged_prs: &[MergedPullRequest]) -> Self {
        Self {
            branches: local
                .iter()
                .map(|b| (b.name.clone(), b.target.clone()))
                .collect(),
            merged_prs: merged_prs
                .iter()
                .filter(|p| p.base == bases.name)
                .cloned()
                .collect(),
            bases,
        }
    }
}

/// What a read of these would depend on, as a number: checked after every read of the
/// repository, which mostly changes nothing here, without copying the branches.
pub(crate) fn key_of(local: &[Branch], bases: &Bases, merged_prs: &[MergedPullRequest]) -> u64 {
    let mut h = DefaultHasher::new();
    local.len().hash(&mut h);
    for b in local {
        (&b.name, &b.target).hash(&mut h);
    }
    bases.hash(&mut h);
    merged_prs.len().hash(&mut h);
    for p in merged_prs {
        (&p.branch, &p.sha, p.number, &p.base).hash(&mut h);
    }
    h.finish()
}

/// `git cherry` answers, by `(branch tip, base commit)`: whether all its commits are in.
pub(crate) type CherryCache = HashMap<(String, String), bool>;

/// Reads, with git, what [`Health`] holds; `cache` holds earlier `git cherry` answers and
/// comes back with this read's. Runs in the background.
pub(crate) fn read(
    repo: &Path,
    inputs: &Inputs,
    cache: &mut CherryCache,
) -> Result<Health, String> {
    let dated = git::branches_by_date(repo)?;
    let tips: HashMap<&str, &str> = inputs
        .branches
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
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
    let open: Vec<&str> = dated
        .iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| *n != inputs.bases.name && !merged.contains_key(*n))
        .collect();
    let found = rebased_or_squashed(repo, inputs, &open, &tips, cache);
    merged.extend(found);
    // Each verdict with the tip the tab saw (what its refs say): one that moved since this
    // read began is not known to be merged.
    let merged = merged
        .into_iter()
        .filter_map(|(name, how)| {
            let tip = (*tips.get(name.as_str())?).to_owned();
            Some((name, (tip, how)))
        })
        .collect();
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
    cache: &mut CherryCache,
) -> HashMap<String, Merged> {
    let mut found = HashMap::new();
    let mut checked: HashSet<&str> = HashSet::new();
    for &name in open {
        let Some(tip) = tips.get(name) else { continue };
        let pr = inputs
            .merged_prs
            .iter()
            .find(|p| p.branch == name && git::is_ancestor(repo, tip, &p.sha));
        if let Some(pr) = pr {
            found.insert(name.to_owned(), Merged::Pr(pr.number, pr.base.clone()));
            checked.insert(name);
        }
    }
    let Some((base, base_tip)) = inputs.bases.refs.first() else {
        return found;
    };
    let short = short_ref(base);
    let mut answers = CherryCache::new();
    let unchecked = open.iter().filter(|n| !checked.contains(*n));
    for &name in unchecked.take(CHERRY_MAX) {
        let Some(tip) = tips.get(name) else { continue };
        let key = ((*tip).to_owned(), base_tip.clone());
        let all_in = match cache.get(&key) {
            Some(known) => *known,
            None => match git::cherry_equivalent(repo, base, &format!("refs/heads/{name}")) {
                Ok(answer) => answer,
                // Not kept: asked again next time.
                Err(_) => continue,
            },
        };
        answers.insert(key, all_in);
        if all_in {
            found.insert(name.to_owned(), Merged::Rebased(short.clone()));
        }
    }
    // Only this read's answers: those of branches gone or moved since go.
    *cache = answers;
    found
}

/// `main` for `refs/heads/main`, `origin/main` for `refs/remotes/origin/main`.
fn short_ref(full: &str) -> String {
    full.strip_prefix("refs/heads/")
        .or_else(|| full.strip_prefix("refs/remotes/"))
        .unwrap_or(full)
        .to_owned()
}

/// The reads asked for: one at a time, the latest wanted read after it.
#[derive(Default)]
pub(crate) struct HealthState {
    /// What the last read that came back found.
    pub health: Option<Health>,
    /// The key of the latest read started or done.
    asked: Option<u64>,
    /// A read is running: another one waits for it.
    running: bool,
    /// The inputs changed while one ran: read again when it is back.
    pending: bool,
    /// `git cherry` answers kept from read to read.
    pub cache: CherryCache,
    /// The merged pull requests of the repository on the forge.
    pub merged_prs: Vec<MergedPullRequest>,
    pub error: Option<String>,
}

impl HealthState {
    /// Whether to start a read for `key` now: not when that was read already, nor while
    /// another runs (it is then read once that one is back). Starts it.
    pub fn should_read(&mut self, key: u64) -> bool {
        if self.asked == Some(key) {
            return false;
        }
        if self.running {
            self.pending = true;
            return false;
        }
        self.asked = Some(key);
        self.running = true;
        true
    }

    /// Takes in a read's answer; whether another read is wanted now.
    pub fn arrived(&mut self, result: Result<(Health, CherryCache), String>) -> bool {
        self.running = false;
        match result {
            Ok((health, cache)) => {
                self.health = Some(health);
                self.cache = cache;
                self.error = None;
            }
            Err(e) => {
                // Asked again on the next change or reload, rather than never.
                self.asked = None;
                self.error = Some(crate::i18n::message(
                    "repo-core-0246",
                    &[("e", e.to_string().into())],
                ));
            }
        }
        std::mem::take(&mut self.pending)
    }

    /// Whether a read is running, or none came back yet.
    pub fn checking(&self) -> bool {
        self.running || (self.health.is_none() && self.error.is_none())
    }

    /// The repository on the forge changed: its merged pull requests, and what they said, are
    /// another repository's.
    pub fn forget_prs(&mut self) {
        self.merged_prs.clear();
        if let Some(health) = &mut self.health {
            health.forget_prs();
        }
        self.asked = None;
    }
}

#[cfg(test)]
#[path = "direct_tests/branch_health_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "direct_tests/branch_health_fluent_reason_tests.rs"]
mod fluent_reason_tests;
