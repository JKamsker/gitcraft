//! Suggesting the branch a branch is based on, from history: the local branch it forks from
//! most recently.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Recorded, in_parallel, local_ref, would_loop};
use crate::git::{count_apart, run};

/// How many local branches are looked at, the most recently committed to first: each costs a
/// `git merge-base` and a `git rev-list`, which on another machine are round trips.
const MAX_CANDIDATES: usize = 120;
/// [`MAX_CANDIDATES`] for a repository on another machine.
const MAX_CANDIDATES_THERE: usize = 40;

/// The branches pull requests usually go to, never left out of the candidates.
const TRUNKS: [&str; 5] = ["main", "master", "develop", "dev", "trunk"];

/// A local branch the branch could be based on, and how far apart the two are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    /// Commits the branch has and the candidate hasn't.
    pub own: u32,
    /// Commits the candidate has and the branch hasn't.
    pub behind: u32,
    /// The candidate is a layer of a stack itself.
    pub in_stack: bool,
}

/// The parent to suggest for a branch among `candidates`: the one it forks from last (fewest
/// commits of its own on top), then the one that moved on least, then a layer of a stack over a
/// branch that isn't, then not a trunk, then by name. A candidate that contains the branch
/// (it has every commit of it, and more) is not its parent but based on it.
pub fn pick_parent(candidates: &[Candidate]) -> Option<&Candidate> {
    candidates
        .iter()
        .filter(|c| !(c.own == 0 && c.behind > 0))
        .min_by(|a, b| {
            (a.own, a.behind, !a.in_stack, is_trunk(&a.name), &a.name).cmp(&(
                b.own,
                b.behind,
                !b.in_stack,
                is_trunk(&b.name),
                &b.name,
            ))
        })
}

fn is_trunk(name: &str) -> bool {
    TRUNKS.contains(&name)
}

/// The local branch `branch` is most likely based on, found from history (see
/// [`pick_parent`]); `None` when no other branch shares a commit with it. Branches based on
/// `branch` (as recorded, or made from it, as their reflog says) are never suggested: that
/// would make a loop.
pub fn suggest_parent(
    repo: &Path,
    branch: &str,
    recorded: &BTreeMap<String, Recorded>,
) -> Result<Option<String>, String> {
    let names: Vec<String> = recent_branches(repo)?
        .into_iter()
        .filter(|name| name != branch && !would_loop(recorded, branch, name))
        .collect();
    // Side by side: on another machine each is a round trip or two.
    let mut candidates: Vec<Candidate> = in_parallel(&names, |name| {
        // No commit in common: an unrelated history, never a parent.
        if !shares_history(repo, name, branch) {
            return None;
        }
        let (behind, own) = count_apart(repo, &local_ref(name), &local_ref(branch)).ok()?;
        Some(Candidate {
            in_stack: recorded.contains_key(name),
            name: name.clone(),
            own,
            behind,
        })
    })
    .into_iter()
    .flatten()
    .collect();
    // Both moved on since they forked, a branch made from `branch` looks like its parent:
    // the best few are asked where they were made from.
    for _ in 0..CREATION_CHECKS {
        let Some(best) = pick_parent(&candidates).map(|c| c.name.clone()) else {
            return Ok(None);
        };
        if created_from(repo, &best).as_deref() != Some(branch) {
            return Ok(Some(best));
        }
        candidates.retain(|c| c.name != best);
    }
    Ok(pick_parent(&candidates).map(|c| c.name.clone()))
}

/// How many of the best candidates are checked for having been made from the branch.
const CREATION_CHECKS: usize = 4;

/// The branch `name` was made from, as its reflog says (`branch: Created from api`), when it
/// says so by a branch's name.
fn created_from(repo: &Path, name: &str) -> Option<String> {
    let out = run(
        Some(repo),
        &["reflog", "show", "--format=%gs", &local_ref(name), "--"],
    )
    .ok()?;
    let from = out
        .lines()
        .last()?
        .strip_prefix("branch: Created from ")?
        .trim();
    Some(from.strip_prefix("refs/heads/").unwrap_or(from).to_owned())
}

/// Whether the two branches have a commit in common.
fn shares_history(repo: &Path, a: &str, b: &str) -> bool {
    crate::git::merge_base(repo, &local_ref(a), &local_ref(b)).is_ok_and(|m| !m.is_empty())
}

/// The local branches, the most recently committed to first, at most [`MAX_CANDIDATES`] of
/// them (fewer elsewhere) besides the trunks.
fn recent_branches(repo: &Path) -> Result<Vec<String>, String> {
    let out = run(
        Some(repo),
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)",
            "refs/heads",
        ],
    )?;
    let all: Vec<String> = out
        .lines()
        .filter_map(|l| l.strip_prefix("refs/heads/"))
        .map(str::to_owned)
        .collect();
    let max = if crate::remote::is_remote(repo) {
        MAX_CANDIDATES_THERE
    } else {
        MAX_CANDIDATES
    };
    let mut names: Vec<String> = all.iter().take(max).cloned().collect();
    for trunk in all.iter().skip(max) {
        if is_trunk(trunk) {
            names.push(trunk.clone());
        }
    }
    Ok(names)
}
