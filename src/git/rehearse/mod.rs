//! Rehearsing a rebase or a merge: played through in a scratch worktree on the repository's
//! machine, where it stops (which commit, which files conflict) or what it gives, and whether
//! the test command passes on that; the user's branches and checkout are never touched.

#[cfg(test)]
mod tests;

use std::path::Path;

use super::testlane::{self, RunStop, Scratch, Verdict};
use crate::git::run_ok_codes;
use crate::remote;

/// What is rehearsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RehearseKind {
    /// The branch rebased onto the other.
    Rebase,
    /// The other merged into the branch.
    Merge,
}

/// A rehearsal to play: `kind` of the branch `branch` (its full ref) with `other` (a full ref
/// or a commit), and the test command to run on the result, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RehearseSpec {
    pub kind: RehearseKind,
    pub branch: String,
    pub other: String,
    pub command: Option<String>,
}

/// How the operation went in the scratch worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Played {
    /// Nothing to do: the branch has the other already.
    UpToDate,
    /// The merge only moves the branch forward, to `tip`.
    FastForward { tip: String, tree: String },
    /// It went through: the branch would be at `tip`, `commits` replayed (1 for a merge).
    Clean {
        tip: String,
        tree: String,
        commits: usize,
    },
    /// It stops on conflicts at step `step` of `of` (a rebase's commits; 1 of 1 for a merge),
    /// at `commit` (the rebase's commit being replayed).
    Stopped {
        step: usize,
        of: usize,
        commit: Option<String>,
        summary: String,
        conflicts: Vec<String>,
    },
    /// It failed for another reason (a hook refused, …).
    Failed(String),
}

impl Played {
    /// The tree it gives, when it goes through.
    pub fn tree(&self) -> Option<&str> {
        match self {
            Played::FastForward { tree, .. } | Played::Clean { tree, .. } => Some(tree),
            _ => None,
        }
    }
}

/// What the test command said of the result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tested {
    /// It ran: its verdict, exit code and last lines.
    Ran {
        verdict: Verdict,
        code: Option<i32>,
        lines: Vec<String>,
    },
    /// The result's tree has a stored result already.
    Known(Verdict),
    /// It wasn't run, and why.
    NotRun(String),
}

/// A rehearsal played: from what (the branch's and the other's commits then), how it went,
/// and the test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rehearsal {
    pub spec: RehearseSpec,
    pub branch_tip: String,
    pub other_tip: String,
    pub played: Played,
    pub tested: Option<Tested>,
}

/// What scratch commits are made with: never signed (no key prompt for a rehearsal), never
/// with an editor.
const SCRATCH_CONFIG: [&str; 2] = ["-c", "commit.gpgSign=false"];

/// Who scratch commits are made as when git knows nobody.
const FALLBACK_IDENTITY: [&str; 4] = [
    "-c",
    "user.name=gitcraft",
    "-c",
    "user.email=gitcraft@localhost",
];

/// Plays `spec` in a scratch worktree of `repo` and runs the test there, until done or `stop`
/// stops it. For a background thread. The worktree is removed in the end, however it ends.
pub fn rehearse(repo: &Path, spec: &RehearseSpec, stop: &RunStop) -> Result<Rehearsal, String> {
    let branch_tip = resolve(repo, &spec.branch)?;
    let other_tip = resolve(repo, &spec.other)?;
    let scratch = Scratch::add_empty(repo, "rehearse", &branch_tip)?;
    scratch.checkout(&branch_tip)?;
    let mut pre: Vec<&str> = SCRATCH_CONFIG.to_vec();
    if !testlane::has_committer(repo) {
        pre.extend(FALLBACK_IDENTITY);
    }
    let played = match spec.kind {
        RehearseKind::Rebase => play_rebase(&scratch.path, &pre, &branch_tip, &other_tip),
        RehearseKind::Merge => play_merge(&scratch.path, &pre, &branch_tip, &other_tip),
    };
    let tested = spec
        .command
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .map(|command| test_result(repo, &scratch.path, command, &played, stop));
    scratch.remove()?;
    if stop.stopped() {
        return Err("stopped".to_owned());
    }
    Ok(Rehearsal {
        spec: spec.clone(),
        branch_tip,
        other_tip,
        played,
        tested,
    })
}

/// The commit `rev` names, or why it names none.
fn resolve(repo: &Path, rev: &str) -> Result<String, String> {
    crate::git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "-q",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| format!("{} isn't a commit", short_ref(rev)))
}

/// `refs/heads/main` as people name it: `main`.
pub fn short_ref(rev: &str) -> &str {
    rev.strip_prefix("refs/heads/")
        .or_else(|| rev.strip_prefix("refs/remotes/"))
        .or_else(|| rev.strip_prefix("refs/tags/"))
        .unwrap_or(rev)
}

/// git in the scratch worktree `dir`, with `pre` before the subcommand; `ok` exit codes count.
fn scratch_git(dir: &Path, pre: &[&str], args: &[&str], ok: &[i32]) -> Result<String, String> {
    let mut all = pre.to_vec();
    all.extend(args);
    run_ok_codes(Some(dir), &all, ok)
}

fn play_rebase(dir: &Path, pre: &[&str], tip: &str, onto: &str) -> Played {
    let ran = scratch_git(dir, pre, &["rebase", "--no-autostash", onto], &[0, 1]);
    let head = crate::git::head(dir);
    match ran {
        Err(e) => {
            if let Some(stop) = rebase_stop(dir) {
                return stop;
            }
            Played::Failed(e)
        }
        Ok(_) if rebase_in_progress(dir) => rebase_stop(dir)
            .unwrap_or_else(|| Played::Failed("the rebase stopped for no reason git gave".into())),
        Ok(_) if head == tip => Played::UpToDate,
        Ok(_) => clean(dir, &head, onto),
    }
}

/// A clean result at `head`, its commits counted from `base`.
fn clean(dir: &Path, head: &str, base: &str) -> Played {
    let commits = run_ok_codes(
        Some(dir),
        &["rev-list", "--count", &format!("{base}..{head}")],
        &[0],
    )
    .ok()
    .and_then(|n| n.trim().parse().ok())
    .unwrap_or(0);
    match tree_of(dir, head) {
        Ok(tree) => Played::Clean {
            tip: head.to_owned(),
            tree,
            commits,
        },
        Err(e) => Played::Failed(e),
    }
}

fn tree_of(dir: &Path, rev: &str) -> Result<String, String> {
    run_ok_codes(Some(dir), &["rev-parse", &format!("{rev}^{{tree}}")], &[0])
        .map(|s| s.trim().to_owned())
}

/// Whether a rebase is going on in `dir`.
fn rebase_in_progress(dir: &Path) -> bool {
    ["rebase-merge", "rebase-apply"]
        .iter()
        .any(|d| git_path(dir, d).is_some_and(|p| remote::try_is_dir(&p).unwrap_or(false)))
}

/// The path git gives `name` in the git folder of the worktree `dir`.
fn git_path(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let out = run_ok_codes(Some(dir), &["rev-parse", "--git-path", name], &[0]).ok()?;
    Some(remote::resolve(dir, out.trim_end_matches(['\n', '\r'])))
}

/// A number git keeps in a file of the rebase going on (`rebase-merge/msgnum`…).
fn rebase_number(dir: &Path, name: &str) -> Option<usize> {
    let path = git_path(dir, name)?;
    remote::read_to_string(&path).ok()?.trim().parse().ok()
}

/// Where the rebase going on in `dir` stopped, if it stopped on conflicts.
fn rebase_stop(dir: &Path) -> Option<Played> {
    if !rebase_in_progress(dir) {
        return None;
    }
    let step = rebase_number(dir, "rebase-merge/msgnum")
        .or_else(|| rebase_number(dir, "rebase-apply/next"))
        .unwrap_or(0);
    let of = rebase_number(dir, "rebase-merge/end")
        .or_else(|| rebase_number(dir, "rebase-apply/last"))
        .unwrap_or(step);
    let commit = run_ok_codes(
        Some(dir),
        &["rev-parse", "--verify", "-q", "REBASE_HEAD"],
        &[0],
    )
    .ok()
    .map(|s| s.trim().to_owned());
    let summary = commit
        .as_deref()
        .and_then(|c| run_ok_codes(Some(dir), &["log", "-1", "--format=%s", c], &[0]).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    Some(Played::Stopped {
        step,
        of,
        commit,
        summary,
        conflicts: conflicts(dir),
    })
}

/// The files with conflicts in `dir`.
pub fn conflicts(dir: &Path) -> Vec<String> {
    run_ok_codes(
        Some(dir),
        &["diff", "--name-only", "--diff-filter=U", "-z"],
        &[0],
    )
    .map(|out| {
        let mut files: Vec<String> = out
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(str::to_owned)
            .collect();
        files.dedup();
        files
    })
    .unwrap_or_default()
}

fn play_merge(dir: &Path, pre: &[&str], tip: &str, other: &str) -> Played {
    let message = format!("Merge {} (rehearsal)", short(other));
    let ran = scratch_git(
        dir,
        pre,
        &["merge", "--no-edit", "-m", &message, other],
        &[0],
    );
    let head = crate::git::head(dir);
    match ran {
        Ok(_) if head == tip => Played::UpToDate,
        Ok(_) if head == other => match tree_of(dir, &head) {
            Ok(tree) => Played::FastForward { tip: head, tree },
            Err(e) => Played::Failed(e),
        },
        Ok(_) => clean(dir, &head, tip),
        Err(e) => {
            let conflicts = conflicts(dir);
            if conflicts.is_empty() {
                Played::Failed(e)
            } else {
                Played::Stopped {
                    step: 1,
                    of: 1,
                    commit: None,
                    summary: String::new(),
                    conflicts,
                }
            }
        }
    }
}

/// Runs `command` on the result `played` in `dir` (or takes the result stored for its tree),
/// storing what it said as the test lane does.
fn test_result(repo: &Path, dir: &Path, command: &str, played: &Played, stop: &RunStop) -> Tested {
    let tree = match played {
        Played::UpToDate => return Tested::NotRun("nothing changes".to_owned()),
        Played::Stopped { .. } => return Tested::NotRun("it stops on conflicts".to_owned()),
        Played::Failed(_) => return Tested::NotRun("it failed".to_owned()),
        Played::FastForward { tree, .. } | Played::Clean { tree, .. } => tree,
    };
    if let Ok(known) = testlane::read_results(repo, testlane::DEFAULT_TEST)
        && let Some(verdict) = known.get(tree)
        && testlane::test_command(repo, testlane::DEFAULT_TEST)
            .ok()
            .flatten()
            .as_deref()
            == Some(command)
    {
        return Tested::Known(*verdict);
    }
    let stopper = stop.command_stopper();
    match testlane::run_command(dir, command, &stopper) {
        Err(e) => Tested::NotRun(e),
        Ok((ended, lines)) => match testlane::verdict_of(&ended) {
            None => Tested::NotRun("stopped".to_owned()),
            Some(verdict) => {
                // Kept for the lane only when it is the repository's test command.
                let stored = testlane::test_command(repo, testlane::DEFAULT_TEST);
                if stored.ok().flatten().as_deref() == Some(command) {
                    let fallback = !testlane::has_committer(repo);
                    let _ = testlane::write_result(
                        repo,
                        testlane::DEFAULT_TEST,
                        tree,
                        verdict,
                        fallback,
                    );
                }
                Tested::Ran {
                    verdict,
                    code: ended.code,
                    lines,
                }
            }
        },
    }
}

fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// Whether the refs a rehearsal was played from are where they were: `Ok` when they are,
/// else what moved, said for the user.
pub fn unmoved(repo: &Path, rehearsal: &Rehearsal) -> Result<(), String> {
    let spec = &rehearsal.spec;
    for (rev, then) in [
        (&spec.branch, &rehearsal.branch_tip),
        (&spec.other, &rehearsal.other_tip),
    ] {
        let now = resolve(repo, rev).unwrap_or_default();
        if &now != then {
            let now = if now.is_empty() {
                "gone".to_owned()
            } else {
                format!("now at {}", short(&now))
            };
            return Err(format!(
                "{} moved since the rehearsal (it was at {}, {now}): rehearse again",
                short_ref(rev),
                short(then)
            ));
        }
    }
    Ok(())
}
