//! "Remove this, keep later work": an older operation that only made commits on a branch
//! (commits, a merge, cherry-picks) taken out, and the commits made on the branch since played
//! again without it, in a worktree of their own beside the repository, on a new branch. The
//! branch itself doesn't move. A commit that doesn't apply any more stops there, its files
//! shown, with the worktree kept to resolve it in.

use std::path::{Path, PathBuf};

use super::Entry;
use crate::git;

/// Commits listed at most per side in what is shown.
const LISTED: usize = 50;

/// What removing an entry takes out and plays again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The entry it removes.
    pub(crate) id: u64,
    pub(crate) branch: String,
    /// The branch before the entry, after it, and now.
    pub(crate) old: String,
    pub(crate) new: String,
    pub(crate) now: String,
    /// The entry's commits, oldest first: (hash, summary), at most [`LISTED`].
    pub(crate) removed: Vec<(String, String)>,
    pub(crate) removed_total: usize,
    /// The commits made since, played again, oldest first.
    pub(crate) later: Vec<String>,
    /// Their summaries, at most [`LISTED`].
    pub(crate) later_listed: Vec<(String, String)>,
}

/// How playing them again went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The new branch has every later commit.
    Done { branch: String, tip: String },
    /// A later commit doesn't apply: the new branch stops before it, in the worktree `folder`,
    /// the cherry-pick stopped on its conflicts there.
    Conflict {
        branch: String,
        folder: PathBuf,
        /// `-C` argument that reaches it from the repository.
        folder_arg: String,
        commit: String,
        summary: String,
        files: Vec<String>,
        /// Later commits still to play after it.
        left: usize,
        /// Where the new branch stopped: once it moved on, the worktree has work in it.
        stopped_at: String,
    },
}

/// The branch `entry` moved and from where to where, when it is one this can remove: a single
/// branch that only got commits (HEAD aside). Read from the entry alone, for whether to offer it.
pub(crate) fn candidate(entry: &Entry) -> Option<(String, String, String)> {
    let moved: Vec<_> = entry
        .changes()
        .into_iter()
        .filter(|c| c.name != "HEAD")
        .collect();
    match moved.as_slice() {
        [c] if !c.old.is_empty() && !c.new.is_empty() => {
            Some((c.name.clone(), c.old.clone(), c.new.clone()))
        }
        _ => None,
    }
}

/// What removing `entry` from `branch`, now at `now`, takes out and plays again; or why it
/// can't be done.
pub(crate) fn plan(repo: &Path, entry: &Entry, now: &str) -> Result<Plan, String> {
    let Some((branch, old, new)) = candidate(entry) else {
        return Err(crate::i18n::text("repo-workflows-timeline-plan-da23e638").to_owned());
    };
    if now.is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-plan-42c2d675",
            &[("branch", (branch).clone().into())],
        ));
    }
    for commit in [&old, &new] {
        if git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{commit}^{{commit}}")],
        )
        .is_err()
        {
            return Err(crate::i18n::message(
                "repo-workflows-timeline-plan-41296321",
                &[(
                    "crate-repo-short-commit-1",
                    (crate::repo::short(commit)).clone().into(),
                )],
            ));
        }
    }
    if !is_ancestor(repo, &old, &new) {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-plan-09f68a9f",
            &[("branch", (branch).clone().into())],
        ));
    }
    if !is_ancestor(repo, &new, now) {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-plan-4b600f60",
            &[("branch", (branch).clone().into())],
        ));
    }
    let merges = git::run(
        Some(repo),
        &[
            "rev-list",
            "--merges",
            "--max-count=1",
            &format!("{new}..{now}"),
            "--",
        ],
    )?;
    if !merges.trim().is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-plan-a4ff5403",
            &[("branch", (branch).clone().into())],
        ));
    }
    let later: Vec<String> = git::run(
        Some(repo),
        &[
            "rev-list",
            "--reverse",
            "--topo-order",
            &format!("{new}..{now}"),
            "--",
        ],
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    let (removed, removed_total) = listed(repo, &format!("{old}..{new}"))?;
    let (later_listed, _) = listed(repo, &format!("{new}..{now}"))?;
    Ok(Plan {
        id: entry.id,
        branch,
        old,
        new,
        now: now.to_owned(),
        removed,
        removed_total,
        later,
        later_listed,
    })
}

fn is_ancestor(repo: &Path, a: &str, b: &str) -> bool {
    git::run(Some(repo), &["merge-base", "--is-ancestor", a, b]).is_ok()
}

/// The commits of `range`, oldest first, at most [`LISTED`]: (hash, summary); and how many.
fn listed(repo: &Path, range: &str) -> Result<(Vec<(String, String)>, usize), String> {
    let out = git::run(
        Some(repo),
        &[
            "log",
            "--reverse",
            "--topo-order",
            "--no-show-signature",
            "--format=%H%x1f%s",
            range,
            "--",
        ],
    )?;
    let all: Vec<(String, String)> = out
        .lines()
        .filter_map(|l| l.split_once('\x1f'))
        .map(|(h, s)| (h.to_owned(), s.to_owned()))
        .collect();
    let total = all.len();
    Ok((all.into_iter().take(LISTED).collect(), total))
}

/// The folder of the worktree for a new branch `name`, beside the repository: as a `-C`
/// argument from it (`../repo-name`), and as a path here.
pub(crate) fn folder(repo: &Path, name: &str) -> (String, PathBuf) {
    let base = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_owned());
    let safe: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect();
    let folder = format!("{base}-{safe}");
    let path = match repo.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => crate::remote::join(parent, &folder),
        _ => crate::remote::join(repo, &format!("../{folder}")),
    };
    (format!("../{folder}"), path)
}

/// Plays `plan`'s later commits again on its branch as it was before the entry, on the new
/// branch `name`, in a worktree beside the repository. Done, the worktree goes and the branch
/// stays; stopped on a conflict, both stay, to resolve there.
pub(crate) fn replay(repo: &Path, plan: &Plan, name: &str) -> Result<Outcome, String> {
    let (arg, path) = folder(repo, name);
    let tip = |rev: &str| {
        git::run(Some(repo), &["rev-parse", "--verify", "-q", rev])
            .map(|s| s.trim().to_owned())
            .ok()
    };
    if tip(&format!("refs/heads/{name}")).is_some() {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-replay-5fa33495",
            &[("name", (name).clone().into())],
        ));
    }
    // What was shown is what is played: commits made on the branch since would be left out.
    if tip(&format!("refs/heads/{}", plan.branch)).as_deref() != Some(plan.now.as_str()) {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-replay-abfe0bc2",
            &[("plan-branch-1", (plan.branch).clone().into())],
        ));
    }
    if let Err(e) = git::run(
        Some(repo),
        &["worktree", "add", "-q", "-b", name, "--", &arg, &plan.old],
    ) {
        // git makes the branch before the folder (and a hook may fail after both): neither
        // is left behind. The branch didn't exist before: it is this attempt's.
        let _ = git::run(Some(repo), &["worktree", "remove", "--force", "--", &arg]);
        let _ = git::run(Some(repo), &["branch", "-D", "--", name]);
        return Err(crate::i18n::message(
            "repo-workflows-timeline-replay-fddf601c",
            &[("arg", (arg).clone().into()), ("e", (e).clone().into())],
        ));
    }
    let in_worktree = |args: &[&str]| -> Result<String, String> {
        let mut full = vec!["-C", arg.as_str()];
        full.extend_from_slice(args);
        git::run(Some(repo), &full)
    };
    if !plan.later.is_empty() {
        // As a range: one argument however many commits (a command line has a limit).
        let range = format!("{}..{}", plan.new, plan.now);
        let args = [
            "cherry-pick",
            "--allow-empty",
            "--keep-redundant-commits",
            range.as_str(),
        ];
        if let Err(e) = in_worktree(&args) {
            let stopped = in_worktree(&["rev-parse", "--verify", "-q", "CHERRY_PICK_HEAD"])
                .map(|s| s.trim().to_owned())
                .ok()
                .filter(|s| !s.is_empty());
            let Some(commit) = stopped else {
                // Not a conflict: nothing is left behind.
                let _ = git::run(Some(repo), &["worktree", "remove", "--force", "--", &arg]);
                let _ = git::run(Some(repo), &["branch", "-D", "--", name]);
                return Err(crate::i18n::message(
                    "repo-workflows-timeline-replay-d57fcfd1",
                    &[("e", (e).clone().into())],
                ));
            };
            let files = in_worktree(&["diff", "--name-only", "-z", "--diff-filter=U"])
                .unwrap_or_default()
                .split('\0')
                .filter(|f| !f.is_empty())
                .map(str::to_owned)
                .collect();
            let stopped_at = in_worktree(&["rev-parse", "HEAD"])
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            let summary = in_worktree(&["log", "-1", "--format=%s", &commit, "--"])
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            let left = plan
                .later
                .iter()
                .position(|c| *c == commit)
                .map_or(0, |i| plan.later.len() - i - 1);
            return Ok(Outcome::Conflict {
                branch: name.to_owned(),
                folder: path,
                folder_arg: arg,
                commit,
                summary,
                files,
                left,
                stopped_at,
            });
        }
    }
    let tip = in_worktree(&["rev-parse", "HEAD"])?.trim().to_owned();
    git::run(Some(repo), &["worktree", "remove", "--", &arg]).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-timeline-replay-e7c57ae8",
            &[
                ("name", (name).clone().into()),
                ("arg", (arg).clone().into()),
                ("e", (e).clone().into()),
            ],
        )
    })?;
    Ok(Outcome::Done {
        branch: name.to_owned(),
        tip,
    })
}

/// Throws away an attempt that stopped on a conflict: its worktree and its branch; unless work
/// was done there since (the cherry-pick continued or given up, commits made), which this would
/// throw away too.
pub(crate) fn discard(
    repo: &Path,
    folder_arg: &str,
    name: &str,
    stopped_at: &str,
) -> Result<(), String> {
    let in_worktree = |args: &[&str]| {
        let mut full = vec!["-C", folder_arg];
        full.extend_from_slice(args);
        git::run(Some(repo), &full).map(|s| s.trim().to_owned())
    };
    let picking = in_worktree(&["rev-parse", "--verify", "-q", "CHERRY_PICK_HEAD"]).is_ok();
    let branch = git::run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")],
    )
    .map(|s| s.trim().to_owned())
    .unwrap_or_default();
    if !picking || branch != stopped_at {
        return Err(crate::i18n::message(
            "repo-workflows-timeline-discard-02af6d89",
            &[
                ("folder-arg", (folder_arg).clone().into()),
                ("name", (name).clone().into()),
            ],
        ));
    }
    git::run(
        Some(repo),
        &["worktree", "remove", "--force", "--", folder_arg],
    )?;
    git::run(Some(repo), &["branch", "-D", "--", name]).map(|_| ())
}
