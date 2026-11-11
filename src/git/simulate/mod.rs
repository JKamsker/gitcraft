//! What a merge, rebase or cherry-pick would do, worked out without doing it. `git merge-tree
//! --write-tree` merges in the object database only: the working tree, the index and the refs
//! stay as they are, so it can run in the background, on another machine too. It writes the
//! merged trees (and, on git before 2.45, a commit for each step of a replay) as loose objects
//! that nothing points to, which `git gc` removes in time.
//!
//! A rebase or a cherry-pick of several commits is replayed commit by commit, as git does it:
//! each commit merged onto what the one before gave, from its parent. A replay stops at the
//! first commit that conflicts, since what comes after depends on how that is solved.

use std::path::Path;

use super::{END_OPTS, NO_SIGNATURE, UTF8, run, run_code};

/// The most commits a replay goes through; for more, what is after is not checked.
pub const MAX_STEPS: usize = 150;

/// Which simulations the installed git can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// `merge-tree --write-tree` (git 2.38): merges.
    pub merge: bool,
    /// `merge-tree --merge-base` (git 2.40): rebases and cherry-picks.
    pub replay: bool,
    /// Trees as `merge-tree`'s arguments (git 2.45): a replay needs no commit for each step.
    pub trees: bool,
    /// The version, as `git --version` gave it ("2.43.0").
    pub version: (u32, u32, u32),
}

impl Features {
    /// What git `version` can do.
    pub fn of(version: (u32, u32, u32)) -> Self {
        let at_least = |minor: u32| (version.0, version.1) >= (2, minor);
        Self {
            merge: at_least(38),
            replay: at_least(40),
            trees: at_least(45),
            version,
        }
    }

    /// Why a merge can't be checked, if it can't.
    pub fn merge_unavailable(&self) -> Option<String> {
        (!self.merge).then(|| self.too_old("2.38"))
    }

    /// Why a rebase or cherry-pick can't be checked, if it can't.
    pub fn replay_unavailable(&self) -> Option<String> {
        (!self.replay).then(|| self.too_old("2.40"))
    }

    fn too_old(&self, needed: &str) -> String {
        let (major, minor, patch) = self.version;
        crate::i18n::message(
            "backend-conflict-check-needs-git-needed-or-newer-this-is-major-minor-patc-7f112b",
            &[
                ("needed", (needed).to_string().into()),
                ("major", (major).to_string().into()),
                ("minor", (minor).to_string().into()),
                ("patch", (patch).to_string().into()),
            ],
        )
    }
}

/// The version in what `git --version` prints: `git version 2.43.0`,
/// `git version 2.45.1.windows.1`, `git version 2.39.3 (Apple Git-145)`.
pub fn parse_version(out: &str) -> Option<(u32, u32, u32)> {
    let word = out
        .trim()
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut parts = word.split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

/// What the git of `repo`'s machine can simulate.
pub fn features(repo: &Path) -> Result<Features, String> {
    let out = run(Some(repo), &["--version"])?;
    parse_version(&out).map(Features::of).ok_or_else(|| {
        crate::i18n::message(
            "backend-unrecognised-git-version-98530a",
            &[("detail", (out.trim()).to_string().into())],
        )
    })
}

/// Whether `repo` is a partial clone (`git clone --filter`): merging there downloads the
/// files it lacks from the remote, which a check must not do behind the user's back.
pub fn partial_clone(repo: &Path) -> Result<bool, String> {
    // 1: no such key. Any other failure (a machine out of reach) is not a "no".
    let (code, out) = run_code(
        Some(repo),
        &["config", "--get", "extensions.partialClone"],
        &[0, 1],
    )?;
    if code == 0 && !String::from_utf8_lossy(&out).trim().is_empty() {
        return Ok(true);
    }
    // A remote marked as a promisor lets git fetch what is missing too.
    let (code, out) = run_code(
        Some(repo),
        &[
            "config",
            "--type=bool",
            "--get-regexp",
            r"^remote\..*\.promisor$",
        ],
        &[0, 1],
    )?;
    Ok(code == 0
        && String::from_utf8_lossy(&out)
            .lines()
            .any(|line| line.ends_with(" true")))
}

/// A commit to replay: its hash, its first parent (none for a root commit) and its summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub hash: String,
    pub parent: Option<String>,
    pub summary: String,
}

/// The commit of a replay that conflicts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    /// Where it is in the replay, from 1.
    pub index: usize,
    pub commit: Pick,
}

/// How an operation would go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// No conflicts; `changes` is false when it would change nothing (all already there).
    Clean { changes: bool },
    /// It would stop with these files conflicted (at `stop`, for a replay).
    Conflicts {
        files: Vec<ConflictFile>,
        stop: Option<Stop>,
    },
    /// The part checked is clean, but not all of it could be: why.
    Unsure(String),
}

/// What a merge, rebase or cherry-pick would do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prediction {
    pub outcome: Outcome,
    /// How many commits it replays (0 for a merge).
    pub commits: usize,
}

/// Merging `theirs` into `ours` (`git merge theirs` with `ours` checked out).
pub fn merge(repo: &Path, ours: &str, theirs: &str) -> Result<Prediction, String> {
    let merged = merge_tree(repo, None, ours, theirs)?;
    let outcome = if merged.conflicts.is_empty() {
        // Already merged only when `theirs` is in `ours`' history: a branch with the same
        // files but its own commits still makes a merge commit.
        let changes = !is_ancestor(repo, theirs, ours)? || tree_of(repo, ours)? != merged.tree;
        Outcome::Clean { changes }
    } else {
        Outcome::Conflicts {
            files: merged.conflicts,
            stop: None,
        }
    };
    Ok(Prediction {
        outcome,
        commits: 0,
    })
}

/// Rebasing `branch` onto `onto` (`git rebase onto` with `branch` checked out): the commits
/// `git rebase` would pick, replayed one by one.
pub fn rebase(
    repo: &Path,
    features: Features,
    branch: &str,
    onto: &str,
) -> Result<Prediction, String> {
    let picks = rebase_picks(repo, branch, onto)?;
    finish_rebase(repo, features, branch, onto, &picks)
}

/// Pulling with `--rebase` into `branch` from `upstream` (the ref, as `origin/main`), at
/// commit `onto`: like `git pull`, the commits replayed start from the upstream's fork point,
/// so commits the upstream once had and then lost (a force-pushed upstream) are not replayed.
pub fn pull_rebase(
    repo: &Path,
    features: Features,
    branch: &str,
    upstream: &str,
    onto: &str,
) -> Result<Prediction, String> {
    let picks = match fork_point(repo, upstream, branch) {
        Some(fork) => picks_since(repo, branch, upstream, &fork)?,
        None => rebase_picks(repo, branch, onto)?,
    };
    finish_rebase(repo, features, branch, onto, &picks)
}

/// Replays `picks` of `branch` onto `onto`, unless the user's git recreates merges in a
/// rebase (`rebase.rebaseMerges`) and there are merges to recreate, which this doesn't
/// simulate.
fn finish_rebase(
    repo: &Path,
    features: Features,
    branch: &str,
    onto: &str,
    picks: &[Pick],
) -> Result<Prediction, String> {
    if rebases_merges(repo) && has_merges(repo, branch, onto)? {
        return Ok(Prediction {
            outcome: Outcome::Unsure(
                crate::i18n::text("backend-rebase-rebasemerges-is-set-and-the-merges-it-would-recreate-aren-5ad5d3")
                    .to_owned(),
            ),
            commits: picks.len(),
        });
    }
    let replay = replay(repo, features, onto, picks)?;
    Ok(replay.prediction(picks.len()))
}

/// Whether `ancestor` is in `of`'s history.
fn is_ancestor(repo: &Path, ancestor: &str, of: &str) -> Result<bool, String> {
    let (code, _) = run_code(
        Some(repo),
        &["merge-base", "--is-ancestor", END_OPTS, ancestor, of],
        &[0, 1],
    )?;
    Ok(code == 0)
}

/// Where `branch` forked from `upstream`, by the upstream's reflog (`merge-base
/// --fork-point`), if git can tell.
fn fork_point(repo: &Path, upstream: &str, branch: &str) -> Option<String> {
    run(
        Some(repo),
        &["merge-base", "--fork-point", END_OPTS, upstream, branch],
    )
    .ok()
    .map(|s| s.trim().to_owned())
    .filter(|s| !s.is_empty())
}

/// The commits of `branch` after `base` that `upstream` lacks, oldest first, less merges:
/// what `git rebase --fork-point` picks (`upstream...branch ^base`, so commits a merge
/// brought in from `upstream` aren't replayed again).
fn picks_since(repo: &Path, branch: &str, upstream: &str, base: &str) -> Result<Vec<Pick>, String> {
    let range = format!("{upstream}...{branch}");
    let exclude = format!("^{base}");
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "--format=%H%x00%P%x00%s%x00",
            "--reverse",
            "--topo-order",
            "--right-only",
            "--cherry-pick",
            "--no-merges",
            END_OPTS,
            &range,
            &exclude,
        ],
    )?;
    Ok(parse_picks(&out))
}

/// Whether the user's git config has rebases recreate merges.
fn rebases_merges(repo: &Path) -> bool {
    run(Some(repo), &["config", "--get", "rebase.rebaseMerges"]).is_ok_and(|v| {
        let v = v.trim().to_ascii_lowercase();
        !v.is_empty() && !["false", "no", "off", "0"].contains(&v.as_str())
    })
}

/// Whether `branch` has merges `onto` doesn't.
fn has_merges(repo: &Path, branch: &str, onto: &str) -> Result<bool, String> {
    let range = format!("{onto}..{branch}");
    let out = run(
        Some(repo),
        &["rev-list", "--merges", "--count", END_OPTS, &range],
    )?;
    Ok(out.trim() != "0")
}

/// Cherry-picking `commits`, oldest first, onto `onto`.
pub fn cherry_pick(
    repo: &Path,
    features: Features,
    onto: &str,
    commits: &[&str],
) -> Result<Prediction, String> {
    let picks = commits
        .iter()
        .map(|c| pick(repo, c))
        .collect::<Result<Vec<_>, _>>()?;
    let replay = replay(repo, features, onto, &picks)?;
    Ok(replay.prediction(picks.len()))
}

/// The commits `git rebase onto` picks with `branch` checked out, oldest first: those of
/// `branch` not in `onto`, less merges and those whose change `onto` already has.
pub fn rebase_picks(repo: &Path, branch: &str, onto: &str) -> Result<Vec<Pick>, String> {
    let range = format!("{onto}...{branch}");
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "--format=%H%x00%P%x00%s%x00",
            "--reverse",
            "--topo-order",
            "--right-only",
            "--cherry-pick",
            "--no-merges",
            END_OPTS,
            &range,
        ],
    )?;
    Ok(parse_picks(&out))
}

/// The commit `rev` as a [`Pick`].
fn pick(repo: &Path, rev: &str) -> Result<Pick, String> {
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "-1",
            "--format=%H%x00%P%x00%s%x00",
            END_OPTS,
            rev,
        ],
    )?;
    parse_picks(&out).into_iter().next().ok_or_else(|| {
        crate::i18n::message(
            "backend-rev-is-not-a-commit-081a0f",
            &[("rev", (rev).to_string().into())],
        )
    })
}

/// Commits from `%H%x00%P%x00%s%x00` records.
fn parse_picks(out: &str) -> Vec<Pick> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|[hash, parents, summary]| {
            let hash = hash.trim().to_owned();
            if hash.is_empty() {
                return None;
            }
            Some(Pick {
                hash,
                parent: parents.split_whitespace().next().map(str::to_owned),
                summary: (*summary).to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;

mod replay;
#[allow(unused_imports)] // Compatibility re-export.
pub use replay::Replay;
pub use replay::replay;
use replay::tree_of;

mod merge_tree;
pub use merge_tree::ConflictFile;
#[allow(unused_imports)] // Compatibility re-export.
pub use merge_tree::TreeMerge;
pub use merge_tree::merge_tree;
#[allow(unused_imports)] // Compatibility re-export.
pub use merge_tree::parse_merge_tree;
