//! Commits replayed onto another one, as a rebase (or a cherry-pick) does it, without touching
//! the repository: `git replay` when the git there has it and the commits are a plain range,
//! else `git merge-tree` a commit at a time, else a scratch clone.

use std::collections::HashMap;
use std::path::Path;

use super::super::{END_OPTS, NO_SIGNATURE, run, run_ok_codes};
use super::Failure;
use super::{CANCELLED, Engine, GitVersion, commit_oid, is_oid, scratch, tree_of, with_write_opts};

/// One commit replayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replayed {
    /// The commit as it is now.
    pub original: String,
    /// The new commit it becomes; `None` when it became empty (what it changes is there
    /// already), which a rebase drops. Made in the repository's object store, except by
    /// [`Engine::Scratch`], whose copies were only in the scratch clone.
    pub copy: Option<String>,
    /// The tree after it.
    pub tree: String,
}

/// Where a replay stops on a conflict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    /// Which of the commits given (from 0).
    pub step: usize,
    pub commit: String,
    /// The files in conflict.
    pub files: Vec<String>,
}

/// The commits replayed, up to the one that conflicts, if one does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    /// The commit they were replayed onto.
    pub onto: String,
    pub steps: Vec<Replayed>,
    pub stop: Option<Stop>,
    pub engine: Engine,
}

impl Replay {
    /// Where the branch ends up: the last copy kept, or `onto` when none was.
    pub fn tip(&self) -> &str {
        self.steps
            .iter()
            .rev()
            .find_map(|s| s.copy.as_deref())
            .unwrap_or(&self.onto)
    }
}

/// The commits `git rebase <upstream>` replays of `tip`, oldest first: those `upstream`
/// hasn't, merges and commits whose change `upstream` has already (cherry-picked) left out.
/// As git's sequencer does, an empty commit is kept even when `upstream` has one too (two
/// empty commits have the same, empty, patch).
pub fn rebase_picks(repo: &Path, upstream: &str, tip: &str) -> Result<Vec<String>, String> {
    let marked = run(
        Some(repo),
        &[
            "rev-list",
            "--reverse",
            "--topo-order",
            "--no-merges",
            "--right-only",
            "--cherry-mark",
            END_OPTS,
            &format!("{upstream}...{tip}"),
        ],
    )?;
    let mut picks = Vec::new();
    for line in marked.lines() {
        let line = line.trim();
        if let Some(hash) = line.strip_prefix('+') {
            picks.push(hash.to_owned());
        } else if let Some(hash) = line.strip_prefix('=')
            && started_empty(repo, hash)
        {
            picks.push(hash.to_owned());
        }
    }
    Ok(picks)
}

/// Whether the commit `hash` changes nothing (its tree is its first parent's).
fn started_empty(repo: &Path, hash: &str) -> bool {
    match (tree_of(repo, hash), tree_of(repo, &format!("{hash}^"))) {
        (Ok(own), Ok(parent)) => own == parent,
        _ => false,
    }
}

fn lines(out: String) -> Result<Vec<String>, String> {
    Ok(out.lines().map(|l| l.trim().to_owned()).collect())
}

/// Compatibility String interface; cached previews use [`replay_with_witness`].
pub fn replay(
    repo: &Path,
    onto: &str,
    base_tip: (&str, &str),
    picks: &[String],
    version: GitVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<Replay, String> {
    replay_with_witness(repo, onto, base_tip, picks, version, cancel)
        .map_err(|failure| failure.display())
}

/// Replays `picks` (oldest first) onto `onto`, as a rebase of `tip` (the full name of its
/// branch, `refs/heads/x`) from `base` does. `cancel` is asked between commits; a `true` ends
/// it with [`CANCELLED`].
pub(crate) fn replay_with_witness(
    repo: &Path,
    onto: &str,
    (base, tip): (&str, &str),
    picks: &[String],
    version: GitVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<Replay, Failure> {
    let onto = commit_oid(repo, onto)?;
    if !picks.is_empty() {
        super::not_partial(repo)?;
    }
    if picks.is_empty() {
        return Ok(Replay {
            onto,
            steps: Vec::new(),
            stop: None,
            engine: if version.has_merge_tree() {
                Engine::MergeTree
            } else {
                Engine::Scratch
            },
        });
    }
    let mut first_error = None;
    if version.has_replay() {
        match with_git_replay(repo, &onto, base, tip, picks) {
            Ok(Some(done)) => return Ok(done),
            // A conflict: which commit has it is found a commit at a time below.
            Ok(None) => {}
            Err(e) => first_error = Some(e),
        }
    }
    if cancel() {
        return Err(Failure::Opaque(CANCELLED.to_owned()));
    }
    if version.has_merge_base_option() {
        match stepwise(repo, &onto, picks, cancel) {
            Ok(done) => return Ok(done),
            Err(e) if e.is_cancelled() => return Err(e),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    scratch::replay(repo, &onto, picks, cancel)
        .map_err(|failure| replay_failure(first_error, failure))
}

/// Retain both explicit witnesses while preserving the cancellation escape.
pub(super) fn replay_failure(first: Option<Failure>, failure: Failure) -> Failure {
    match first {
        Some(first) if !failure.is_cancelled() => Failure::ReplayFallback {
            first: Box::new(first),
            scratch: Box::new(failure),
        },
        _ => failure,
    }
}

/// The format [`with_git_replay`] reads commits with: what a copy keeps of its original.
const KEY_FORMAT: &str = "--format=%H%x00%T%x00%at%x00%ae%x00%s%x00";

/// A commit as [`KEY_FORMAT`] reads it.
struct Keyed {
    hash: String,
    tree: String,
    /// Author time and email and the summary, which a copy has as its original does.
    key: (String, String, String),
}

fn keyed(out: &str) -> Vec<Keyed> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<5>()
        .0
        .iter()
        .filter_map(|[hash, tree, at, ae, s]| {
            let hash = hash.trim_start_matches('\n');
            is_oid(hash).then(|| Keyed {
                hash: hash.to_owned(),
                tree: (*tree).to_owned(),
                key: ((*at).to_owned(), (*ae).to_owned(), (*s).to_owned()),
            })
        })
        .collect()
}

/// `git replay --onto <onto> <base>..<tip>`, used only when `picks` are that whole range (no
/// merges in it, nothing left out as already upstream). `Ok(None)`: it conflicts.
fn with_git_replay(
    repo: &Path,
    onto: &str,
    base: &str,
    tip: &str,
    picks: &[String],
) -> Result<Option<Replay>, Failure> {
    if !tip.starts_with("refs/") || base.starts_with('-') {
        return Err(Failure::ReplayNeedsBranch);
    }
    let range = format!("{base}..{tip}");
    let plain = lines(run(
        Some(repo),
        &["rev-list", "--reverse", "--topo-order", END_OPTS, &range],
    )?)?;
    if plain != picks {
        return Err(Failure::NotPlainRange);
    }
    // From git 2.53, replay moves the branch itself unless told only to print the updates
    // (`--ref-action=print`, which an older git doesn't know; it ignores the config).
    let out = run_ok_codes(
        Some(repo),
        &with_write_opts(&[
            "-c",
            "replay.refAction=print",
            "replay",
            "--onto",
            onto,
            &range,
        ]),
        &[0, 1],
    )?;
    let Some(new_tip) = out.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("update") && words.next() == Some(tip))
            .then(|| words.next().map(str::to_owned))
            .flatten()
    }) else {
        return Ok(None);
    };
    let read = |revs: &[&str]| -> Result<Vec<Keyed>, String> {
        let mut args = vec![
            "log",
            NO_SIGNATURE,
            "--reverse",
            "--topo-order",
            KEY_FORMAT,
            END_OPTS,
        ];
        args.extend(revs);
        args.push("--");
        Ok(keyed(&run(Some(repo), &args)?))
    };
    let originals = read(&[&range])?;
    let copies = read(&[&new_tip, &format!("^{onto}")])?;
    let first_parent_tree = match originals.first() {
        Some(first) => tree_of(repo, &format!("{}^", first.hash))?,
        None => return Err(Failure::NothingReplayed),
    };
    let mut steps = Vec::with_capacity(originals.len());
    let mut tree = tree_of(repo, onto)?;
    let mut next = copies.iter().peekable();
    for (i, original) in originals.iter().enumerate() {
        let parent_tree = match i {
            0 => first_parent_tree.as_str(),
            _ => originals[i - 1].tree.as_str(),
        };
        let started_empty = original.tree == parent_tree;
        let copy = next.next_if(|c| c.key == original.key);
        let kept = copy.filter(|c| started_empty || c.tree != tree);
        if let Some(c) = kept {
            tree.clone_from(&c.tree);
        }
        steps.push(Replayed {
            original: original.hash.clone(),
            copy: kept.map(|c| c.hash.clone()),
            tree: tree.clone(),
        });
    }
    if next.peek().is_some() {
        return Err(Failure::ReplayMismatch);
    }
    Ok(Some(Replay {
        onto: onto.to_owned(),
        steps,
        stop: None,
        engine: super::Engine::Replay,
    }))
}

/// Each commit's tree and parents.
fn trees_and_parents(
    repo: &Path,
    hashes: &[String],
) -> Result<HashMap<String, (String, Vec<String>)>, Failure> {
    let mut found = HashMap::new();
    for chunk in hashes.chunks(100) {
        let mut args = vec![
            "log",
            NO_SIGNATURE,
            "--no-walk=unsorted",
            "--format=%H%x00%T%x00%P%x00",
            END_OPTS,
        ];
        args.extend(chunk.iter().map(String::as_str));
        args.push("--");
        let out = run(Some(repo), &args)?;
        let fields: Vec<&str> = out.split('\0').collect();
        for [hash, tree, parents] in fields.as_chunks::<3>().0 {
            let hash = hash.trim_start_matches('\n').to_owned();
            let parents = parents.split_whitespace().map(str::to_owned).collect();
            found.insert(hash, ((*tree).to_owned(), parents));
        }
    }
    Ok(found)
}

/// What `git merge-tree --write-tree -z --name-only` printed: the tree, and the files in
/// conflict (none for a clean merge).
pub(super) fn parse_merge_tree(out: &str) -> Result<(String, Vec<String>), Failure> {
    let mut fields = out.split('\0');
    let tree = fields.next().unwrap_or_default().trim().to_owned();
    if !is_oid(&tree) {
        return Err(Failure::MergeTree {
            detail: out.trim().to_owned(),
        });
    }
    let mut files: Vec<String> = Vec::new();
    for name in fields.take_while(|f| !f.is_empty()) {
        if !files.iter().any(|f| f == name) {
            files.push(name.to_owned());
        }
    }
    Ok((tree, files))
}

/// A commit at a time: the three-way merge a cherry-pick is (`merge-tree --merge-base`, git
/// 2.40), then a commit of its tree (`commit-tree`) for the next one to go on.
fn stepwise(
    repo: &Path,
    onto: &str,
    picks: &[String],
    cancel: &dyn Fn() -> bool,
) -> Result<Replay, Failure> {
    let mut info = trees_and_parents(repo, picks)?;
    let parents: Vec<String> = picks
        .iter()
        .filter_map(|p| info.get(p)?.1.first().cloned())
        .filter(|p| !info.contains_key(p))
        .collect();
    info.extend(trees_and_parents(repo, &parents)?);
    let mut current = onto.to_owned();
    let mut tree = tree_of(repo, onto)?;
    let mut steps = Vec::with_capacity(picks.len());
    for (step, commit) in picks.iter().enumerate() {
        if cancel() {
            return Err(Failure::Opaque(CANCELLED.to_owned()));
        }
        let (own_tree, parents) = info.get(commit).ok_or_else(|| Failure::NotCommit {
            commit: commit.to_owned(),
        })?;
        let parent = parents.first().ok_or_else(|| Failure::FirstCommit {
            commit: short(commit).to_owned(),
        })?;
        let out = run_ok_codes(
            Some(repo),
            &[
                "merge-tree",
                "--write-tree",
                "-z",
                "--name-only",
                &format!("--merge-base={parent}"),
                &current,
                commit,
            ],
            &[0, 1],
        )?;
        let (merged, files) = parse_merge_tree(&out)?;
        if !files.is_empty() {
            return Ok(Replay {
                onto: onto.to_owned(),
                steps,
                stop: Some(Stop {
                    step,
                    commit: commit.clone(),
                    files,
                }),
                engine: Engine::MergeTree,
            });
        }
        let started_empty = info.get(parent).is_some_and(|(t, _)| t == own_tree);
        let copy = if merged == tree && !started_empty {
            None
        } else {
            let message = format!("ghost run copy of {commit}");
            let made = run(
                Some(repo),
                &with_write_opts(&["commit-tree", &merged, "-p", &current, "-m", &message]),
            )?;
            let made = made.trim().to_owned();
            if !is_oid(&made) {
                return Err(Failure::CommitTree { made });
            }
            current.clone_from(&made);
            tree.clone_from(&merged);
            Some(made)
        };
        steps.push(Replayed {
            original: commit.clone(),
            copy,
            tree: tree.clone(),
        });
    }
    Ok(Replay {
        onto: onto.to_owned(),
        steps,
        stop: None,
        engine: Engine::MergeTree,
    })
}

fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// Runs only [`stepwise`], for the tests that check it gives what `git replay` and a real
/// rebase give.
#[cfg(test)]
pub(super) fn stepwise_only(repo: &Path, onto: &str, picks: &[String]) -> Result<Replay, String> {
    commit_oid(repo, onto)
        .and_then(|onto| stepwise(repo, &onto, picks, &|| false))
        .map_err(|failure| failure.display())
}
