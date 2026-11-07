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
        format!("Conflict check needs git {needed} or newer (this is {major}.{minor}.{patch})")
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
    parse_version(&out)
        .map(Features::of)
        .ok_or_else(|| format!("Unrecognised git version: {}", out.trim()))
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

/// A file a merge would leave conflicted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictFile {
    pub path: String,
    /// What conflicts in it, as git names it: `contents`, `binary`, `modify/delete`,
    /// `rename/delete`, `add/add`, `file/directory`…
    pub kinds: Vec<String>,
}

impl ConflictFile {
    /// Only its lines conflict (both sides changed the text), so a merge tool working from
    /// the code's syntax may solve it. A binary file, or one deleted or renamed on one side,
    /// is a choice no tool makes.
    pub fn text_only(&self) -> bool {
        !self.kinds.is_empty() && self.kinds.iter().all(|k| k == "contents" || k == "add/add")
    }
}

/// One merge, worked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeMerge {
    /// The merged tree (with conflict markers in it when there are conflicts).
    pub tree: String,
    /// The conflicted files, in git's order; empty when it merges cleanly.
    pub conflicts: Vec<ConflictFile>,
}

/// Merges `ours` and `theirs` (commits; any tree-ish when `base` is given and git is 2.45 or
/// newer), from their merge bases or from `base`.
pub fn merge_tree(
    repo: &Path,
    base: Option<&str>,
    ours: &str,
    theirs: &str,
) -> Result<TreeMerge, String> {
    let base_arg = base.map(|b| format!("--merge-base={b}"));
    let mut args = vec!["merge-tree", "--write-tree", "--name-only", "-z"];
    if let Some(b) = &base_arg {
        args.push(b);
    }
    args.extend([END_OPTS, ours, theirs]);
    // 0: merged cleanly; 1: conflicts. Anything else is an error (an object that isn't there).
    let (code, out) = run_code(Some(repo), &args, &[0, 1])?;
    parse_merge_tree(&out, code == 0)
}

/// What `merge-tree --write-tree --name-only -z` printed: the tree, NUL; on conflicts, each
/// conflicted file NUL-ended, an empty field, then the messages, each as
/// `<count> NUL <path>… NUL <type> NUL <message> NUL`, the type being `CONFLICT (kind)` or
/// something informational (`Auto-merging`).
pub fn parse_merge_tree(out: &[u8], clean: bool) -> Result<TreeMerge, String> {
    let text = String::from_utf8_lossy(out);
    let mut fields = text.split('\0');
    let tree = fields.next().unwrap_or_default().trim().to_owned();
    if tree.is_empty() || !tree.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("Unexpected answer from git merge-tree: {tree:?}"));
    }
    if clean {
        return Ok(TreeMerge {
            tree,
            conflicts: Vec::new(),
        });
    }
    let mut conflicts: Vec<ConflictFile> = fields
        .by_ref()
        .take_while(|f| !f.is_empty())
        .map(|path| ConflictFile {
            path: path.to_owned(),
            kinds: Vec::new(),
        })
        .collect();
    if conflicts.is_empty() {
        return Err("git merge-tree reported conflicts but named no file".to_owned());
    }
    read_messages(fields, &mut conflicts);
    Ok(TreeMerge { tree, conflicts })
}

/// Adds the kind of each `CONFLICT (kind)` message to the conflicted files it names.
fn read_messages<'a>(mut fields: impl Iterator<Item = &'a str>, conflicts: &mut [ConflictFile]) {
    while let Some(count) = fields.next() {
        let Ok(count) = count.parse::<usize>() else {
            return;
        };
        let paths: Vec<&str> = fields.by_ref().take(count).collect();
        let (Some(kind), Some(_message)) = (fields.next(), fields.next()) else {
            return;
        };
        let Some(kind) = kind
            .strip_prefix("CONFLICT (")
            .and_then(|k| k.strip_suffix(')'))
        else {
            continue;
        };
        for file in conflicts
            .iter_mut()
            .filter(|f| paths.contains(&f.path.as_str()))
        {
            if !file.kinds.iter().any(|k| k == kind) {
                file.kinds.push(kind.to_owned());
            }
        }
    }
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
        Some(fork) => picks_since(repo, branch, &fork)?,
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
                "rebase.rebaseMerges is set, and the merges it would recreate aren't checked"
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

/// The commits of `branch` after `base`, oldest first, less merges: what `git rebase --onto
/// <upstream> <base>` picks.
fn picks_since(repo: &Path, branch: &str, base: &str) -> Result<Vec<Pick>, String> {
    let range = format!("{base}..{branch}");
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "--format=%H%x00%P%x00%s%x00",
            "--reverse",
            "--topo-order",
            "--no-merges",
            END_OPTS,
            &range,
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
    parse_picks(&out)
        .into_iter()
        .next()
        .ok_or_else(|| format!("{rev} is not a commit"))
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

/// How a replay went, step by step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    /// The tree after each commit that applied, in order.
    pub trees: Vec<String>,
    /// The tree it started from.
    pub start: String,
    /// The commit that conflicts, and the files, when one does.
    pub stopped: Option<(Stop, Vec<ConflictFile>)>,
    /// Why it stopped before the end without a conflict (a root commit older git can't
    /// replay, more than [`MAX_STEPS`] commits).
    pub unsure: Option<String>,
}

impl Replay {
    /// The outcome of replaying `total` commits.
    fn prediction(self, total: usize) -> Prediction {
        let outcome = match (self.stopped, self.unsure) {
            (Some((stop, files)), _) => Outcome::Conflicts {
                files,
                stop: Some(stop),
            },
            (None, Some(why)) => Outcome::Unsure(why),
            (None, None) => Outcome::Clean {
                changes: self.trees.last().is_some_and(|t| *t != self.start),
            },
        };
        Prediction {
            outcome,
            commits: total,
        }
    }
}

/// Replays `picks` onto `onto` one by one, each merged onto the result so far from its own
/// parent, as a rebase or a cherry-pick does; stops at the first that conflicts.
pub fn replay(
    repo: &Path,
    features: Features,
    onto: &str,
    picks: &[Pick],
) -> Result<Replay, String> {
    if !features.replay {
        return Err(features.replay_unavailable().unwrap_or_default());
    }
    let start = tree_of(repo, onto)?;
    let mut replay = Replay {
        trees: Vec::new(),
        start: start.clone(),
        stopped: None,
        unsure: None,
    };
    // What the next commit is merged onto: a commit, or a tree where git takes one.
    let mut ours = onto.to_owned();
    let mut tree = start;
    for (n, pick) in picks.iter().enumerate() {
        if n == MAX_STEPS {
            replay.unsure = Some(format!(
                "Only the first {MAX_STEPS} of {} commits were checked",
                picks.len()
            ));
            break;
        }
        let base = match &pick.parent {
            Some(parent) => parent.clone(),
            None if features.trees => empty_tree(repo),
            None => {
                replay.unsure = Some(format!(
                    "{} is a root commit, which git {}.{} can't check (2.45 can)",
                    pick.hash.get(..8).unwrap_or(&pick.hash),
                    features.version.0,
                    features.version.1
                ));
                break;
            }
        };
        let merged = merge_tree(repo, Some(&base), &ours, &pick.hash)?;
        if !merged.conflicts.is_empty() {
            let stop = Stop {
                index: n + 1,
                commit: pick.clone(),
            };
            replay.stopped = Some((stop, merged.conflicts));
            break;
        }
        // A commit whose change is already there leaves the tree as it was: a rebase drops it.
        if merged.tree != tree {
            tree = merged.tree.clone();
            ours = if features.trees {
                tree.clone()
            } else {
                scratch_commit(repo, &tree)?
            };
        }
        replay.trees.push(merged.tree);
    }
    Ok(replay)
}

/// The tree of commit `rev`.
fn tree_of(repo: &Path, rev: &str) -> Result<String, String> {
    run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| format!("{rev} is not a commit"))
}

/// A commit of `tree` that nothing points to, for git before 2.45, whose `merge-tree` takes
/// only commits. Never signed; made as `gitcraft` so that no user name needs to be set.
fn scratch_commit(repo: &Path, tree: &str) -> Result<String, String> {
    run(
        Some(repo),
        &[
            "-c",
            "user.name=gitcraft",
            "-c",
            "user.email=gitcraft@localhost",
            "commit-tree",
            "--no-gpg-sign",
            "-m",
            "gitcraft conflict check",
            tree,
        ],
    )
    .map(|s| s.trim().to_owned())
}

/// The empty tree of the repository's hash format.
fn empty_tree(repo: &Path) -> String {
    let format = run(Some(repo), &["rev-parse", "--show-object-format"]).unwrap_or_default();
    match format.trim() {
        "sha256" => "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321",
        _ => "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
    }
    .to_owned()
}

#[cfg(test)]
mod tests;
