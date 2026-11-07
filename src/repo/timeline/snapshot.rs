//! The commits the timeline needs kept from `git gc`, and putting the branches back as an entry
//! had them (a jump in the undo tree, or undoing a change made elsewhere).
//!
//! gitcraft keeps what it needs from gc under one namespace, `refs/gitcraft/<feature>/…`, which
//! the graph leaves out (see `git::log`) and the watcher ignores: the versions of a rewritten
//! branch under `refs/gitcraft/rewrites/`, the timeline's under `refs/gitcraft/undo/`. The timeline's part is
//! `refs/gitcraft/undo/<folder key>/<entry id>`: one ref per entry, at a commit made for it
//! (its tree is that of the first commit kept, and it is never checked out) whose parents are
//! the commits the entry moved branches from and to. The folder key tells the timelines of a
//! repository's worktrees apart (each has its own HEAD).

use std::collections::HashSet;
use std::path::Path;

use super::{Entry, RefState};
use crate::git;
use crate::repo::operations::{expect_ref_at, expect_symref};

/// Where gitcraft keeps refs of its own.
pub(crate) const ROOT: &str = "refs/gitcraft/";
/// Where the timeline's are.
pub(crate) const UNDO: &str = "refs/gitcraft/undo/";

/// The ref keeping the commits of entry `id` of the timeline `key`.
pub(crate) fn ref_name(key: &str, id: u64) -> String {
    format!("{UNDO}{key}/{id}")
}

/// The commits entry `entry` needs to put either side of it back: those of the branches (and
/// of HEAD) it changed, before and after. The others are kept by the branches still at them,
/// or by the entry that later moved them.
pub(crate) fn commits_of(entry: &Entry) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    let mut add = |c: &str| {
        if is_hash(c) && !list.iter().any(|x| x == c) {
            list.push(c.to_owned());
        }
    };
    for change in entry.changes() {
        add(&change.old);
        add(&change.new);
    }
    if entry.before.head != entry.after.head {
        add(&entry.before.head);
        add(&entry.after.head);
    }
    list
}

/// Whether `s` is a full commit id (SHA-1 or SHA-256), not a branch name.
fn is_hash(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Keeps `commits` from gc under the ref of entry `id`. Nothing to keep, nothing done.
pub(crate) fn keep(
    repo: &Path,
    key: &str,
    id: u64,
    label: &str,
    commits: &[String],
) -> Result<(), String> {
    let Some(first) = commits.first() else {
        return Ok(());
    };
    let tree = git::run(
        Some(repo),
        &["rev-parse", "--verify", &format!("{first}^{{tree}}")],
    )?;
    let message = format!("gitcraft timeline: {label}\n\nKeeps the commits of entry {id}.");
    let mut args: Vec<String> = [
        // Its own author: the repository may have none set, and it is no one's work.
        "-c",
        "user.name=gitcraft",
        "-c",
        "user.email=gitcraft@localhost",
        "commit-tree",
        "--no-gpg-sign",
        tree.trim(),
        "-m",
        &message,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for commit in commits {
        args.push("-p".to_owned());
        args.push(commit.clone());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let keeper = git::run(Some(repo), &args)?;
    git::run(
        Some(repo),
        &[
            "update-ref",
            "-m",
            "gitcraft: keep a timeline entry's commits",
            &ref_name(key, id),
            keeper.trim(),
        ],
    )
    .map(|_| ())
}

/// Lets go of the commits of the entries `ids`; they may be collected by gc.
pub(crate) fn release(repo: &Path, key: &str, ids: &[u64]) -> Result<(), String> {
    for id in ids {
        git::run(Some(repo), &["update-ref", "-d", &ref_name(key, *id)])?;
    }
    Ok(())
}

/// Lets go of the refs of the timeline `key` that belong to no entry of `ids` any more
/// (forgotten while gitcraft couldn't, or by another copy of it).
pub(crate) fn release_others(repo: &Path, key: &str, ids: &HashSet<u64>) -> Result<(), String> {
    let prefix = format!("{UNDO}{key}/");
    let listed = git::run(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", &prefix],
    )?;
    let stale: Vec<u64> = listed
        .lines()
        .filter_map(|name| name.strip_prefix(&prefix)?.parse().ok())
        .filter(|id| !ids.contains(id))
        .collect();
    release(repo, key, &stale)
}

/// The git steps that put the branches and HEAD back as `target` has them, from `now` (as the
/// tab last read them): each branch that differs is moved, made or deleted; the branch
/// checked out is moved with `reset --keep`, which keeps uncommitted changes and refuses where
/// they'd be lost; HEAD goes to the branch (or commit) `target` has checked out. Checks come
/// first: nothing runs when any of those branches has moved since it was read.
///
/// `elsewhere`: the branches checked out in other worktrees, which this must not move.
pub(crate) fn restore_steps(
    now: &RefState,
    target: &RefState,
    elsewhere: &[String],
) -> Result<Vec<Vec<String>>, String> {
    let moved: Vec<(&String, Option<&String>, Option<&String>)> = names(now, target)
        .into_iter()
        .filter_map(|name| {
            let (old, new) = (now.branches.get(name), target.branches.get(name));
            (old != new).then_some((name, old, new))
        })
        .collect();
    if let Some((name, ..)) = moved.iter().find(|(n, ..)| elsewhere.contains(n)) {
        return Err(format!(
            "{name} is checked out in another worktree: check it out elsewhere there first"
        ));
    }
    if !target.branch.is_empty() && !target.branches.contains_key(&target.branch) {
        return Err(format!(
            "{} had no commit yet then: pick a later entry, or use New branch there",
            target.branch
        ));
    }
    if target.branch.is_empty() && target.head.is_empty() {
        return Err(
            "Nothing was committed yet then: pick a later entry, or use New branch there"
                .to_owned(),
        );
    }
    if let Some(name) = std::iter::once(&target.branch)
        .chain(moved.iter().map(|(n, ..)| *n))
        .find(|n| n.starts_with('-'))
    {
        return Err(format!(
            "{name} reads like an option to git: rename it first"
        ));
    }
    if !target.branch.is_empty()
        && target.branch != now.branch
        && elsewhere.contains(&target.branch)
    {
        return Err(format!(
            "{} is checked out in another worktree: check it out elsewhere there first",
            target.branch
        ));
    }
    let current = now.branch.as_str();
    let mut steps = vec![expect_symref("HEAD", &full(current))];
    if current.is_empty() {
        steps.push(expect_ref_at("HEAD", &now.head));
    }
    for (name, old, _) in &moved {
        steps.push(expect_ref_at(&full(name), old.map_or("", |s| s.as_str())));
    }
    let update = |name: &str, old: Option<&String>, new: Option<&String>| -> Vec<String> {
        let full = full(name);
        let old = old.map_or("", |s| s.as_str());
        match new {
            Some(new) => strings(&["update-ref", "-m", "gitcraft: undo tree", &full, new, old]),
            None => strings(&["update-ref", "-m", "gitcraft: undo tree", "-d", &full, old]),
        }
    };
    // HEAD goes first, while no branch has moved yet: a checkout refused (local changes in
    // the way) then leaves everything as it was. A branch to check out that isn't there now is
    // made first, at its commit then.
    let same_place = if target.branch.is_empty() {
        current.is_empty() && now.head == target.head
    } else {
        target.branch == current
    };
    let target_moves = moved
        .iter()
        .find(|(n, ..)| !target.branch.is_empty() && **n == target.branch);
    let mut made = false;
    if !same_place {
        if target.branch.is_empty() {
            steps.push(strings(&["checkout", "--detach", &target.head, "--"]));
        } else {
            if let Some((name, None, new)) = target_moves {
                steps.push(update(name, None, *new));
                made = true;
            }
            steps.push(strings(&["checkout", &target.branch, "--"]));
        }
    }
    // Then every other branch, the one checked out before among them.
    for (name, old, new) in &moved {
        if target.branch.is_empty() || **name != target.branch {
            steps.push(update(name, *old, *new));
        }
    }
    // Last, the branch checked out now moves in place, keeping uncommitted changes.
    if let Some((_, Some(_), new)) = target_moves
        && !made
    {
        match new {
            Some(new) => steps.push(strings(&["reset", "--keep", new])),
            None => {
                return Err(format!(
                    "{} didn't exist then, and it is checked out: check out another branch first",
                    target.branch
                ));
            }
        }
    }
    Ok(steps)
}

/// What [`restore_steps`] would change, as a line each ("main: 1a2b3c4d → 5e6f7a8b").
pub(crate) fn describe(now: &RefState, target: &RefState) -> Vec<String> {
    let short = |c: Option<&String>| c.map_or("gone", |c| crate::repo::short(c)).to_owned();
    let mut lines: Vec<String> = names(now, target)
        .into_iter()
        .filter(|n| now.branches.get(*n) != target.branches.get(*n))
        .map(|n| {
            let (old, new) = (now.branches.get(n), target.branches.get(n));
            match (old, new) {
                (None, Some(_)) => format!("{n}: made again at {}", short(new)),
                (Some(_), None) => format!("{n}: deleted (it is kept here, at {})", short(old)),
                _ => format!("{n}: {} → {}", short(old), short(new)),
            }
        })
        .collect();
    let at = |s: &RefState| {
        if s.branch.is_empty() {
            format!("a detached HEAD at {}", crate::repo::short(&s.head))
        } else {
            s.branch.clone()
        }
    };
    if at(now) != at(target) {
        lines.push(format!("Checked out: {} → {}", at(now), at(target)));
    }
    lines
}

/// Every branch of either state, once, in order.
fn names<'a>(a: &'a RefState, b: &'a RefState) -> Vec<&'a String> {
    let mut names: Vec<&String> = a.branches.keys().chain(b.branches.keys()).collect();
    names.sort();
    names.dedup();
    names
}

/// `refs/heads/<name>`, or nothing for no branch (a detached HEAD).
fn full(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("refs/heads/{name}")
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}
