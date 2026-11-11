//! Replay.

use super::*;

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
    pub(super) fn prediction(self, total: usize) -> Prediction {
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
            replay.unsure = Some(crate::i18n::message(
                "backend-only-the-first-max-steps-of-commits-were-checked-797f2a",
                &[
                    ("max_steps", (MAX_STEPS).into()),
                    ("count", (picks.len()).into()),
                ],
            ));
            break;
        }
        let base = match &pick.parent {
            Some(parent) => parent.clone(),
            None if features.trees => empty_tree(repo),
            None => {
                replay.unsure = Some(crate::i18n::message(
                    "backend-is-a-root-commit-which-git-can-t-check-2-45-can-33b417",
                    &[
                        (
                            "pick",
                            (pick.hash.get(..8).unwrap_or(&pick.hash))
                                .to_string()
                                .into(),
                        ),
                        ("features", (features.version.0).to_string().into()),
                        ("features3", (features.version.1).to_string().into()),
                    ],
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
pub(super) fn tree_of(repo: &Path, rev: &str) -> Result<String, String> {
    run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| {
        crate::i18n::message(
            "backend-rev-is-not-a-commit-081a0f",
            &[("rev", (rev).to_string().into())],
        )
    })
}

/// A commit of `tree` that nothing points to, for git before 2.45, whose `merge-tree` takes
/// only commits. Never signed; made as `gitcraft` so that no user name needs to be set.
pub(super) fn scratch_commit(repo: &Path, tree: &str) -> Result<String, String> {
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
pub(super) fn empty_tree(repo: &Path) -> String {
    let format = run(Some(repo), &["rev-parse", "--show-object-format"]).unwrap_or_default();
    match format.trim() {
        "sha256" => "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321",
        _ => "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
    }
    .to_owned()
}
