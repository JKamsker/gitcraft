//! "Where's my work?": what exists in a repository and nowhere else (commits on no remote,
//! stashes, uncommitted changes here or in another worktree, branches whose remote branch is
//! gone), read from a repository's [`Overview`], and pushing a branch from there.

use std::path::{Path, PathBuf};

use crate::git::{self, Overview};

/// One piece of work only this repository has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Work {
    /// Files changed and not committed in the repository's own folder.
    Uncommitted(usize),
    /// A branch with commits its upstream (or, without one, any remote) doesn't have.
    Unpushed {
        branch: String,
        upstream: Option<String>,
        commits: u32,
    },
    /// The stashes, by message, newest first.
    Stashes(Vec<String>),
    /// Another worktree of the repository with uncommitted files.
    Worktree { path: PathBuf, changes: usize },
    /// A branch whose remote branch was deleted: its commits may be nowhere else.
    Gone(String),
}

impl Work {
    /// What the row says.
    pub fn describe(&self) -> String {
        match self {
            Work::Uncommitted(n) => format!("{} with uncommitted changes", files(*n)),
            Work::Unpushed {
                branch,
                upstream: Some(up),
                commits,
            } => format!("{branch}: {} not pushed to {up}", commits_text(*commits)),
            Work::Unpushed {
                branch, commits, ..
            } => format!("{branch}: {} on no remote", commits_text(*commits)),
            Work::Stashes(list) => match list.as_slice() {
                [one] => format!("1 stash: {one}"),
                more => format!("{} stashes, the latest: {}", more.len(), more[0]),
            },
            Work::Worktree { path, changes } => format!(
                "worktree {}: {} with uncommitted changes",
                crate::remote::display(path),
                files(*changes)
            ),
            Work::Gone(branch) => format!("{branch}: its remote branch is gone"),
        }
    }

    /// The icon of its row.
    pub fn icon(&self) -> &'static str {
        match self {
            Work::Uncommitted(_) | Work::Worktree { .. } => "file-diff",
            Work::Unpushed { .. } => "arrow-up",
            Work::Stashes(_) => "archive",
            Work::Gone(_) => "circle-alert",
        }
    }
}

fn files(n: usize) -> String {
    if n == 1 {
        "1 file".to_owned()
    } else {
        format!("{n} files")
    }
}

fn commits_text(n: u32) -> String {
    if n == 1 {
        "1 commit".to_owned()
    } else {
        format!("{n} commits")
    }
}

/// The work `o` says is only in its repository, the most at risk first.
pub fn work_of(o: &Overview) -> Vec<Work> {
    let mut work = Vec::new();
    if o.changed > 0 {
        work.push(Work::Uncommitted(o.changed));
    }
    for w in &o.dirty_worktrees {
        work.push(Work::Worktree {
            path: w.path.clone(),
            changes: w.changes,
        });
    }
    for u in &o.unpushed {
        work.push(Work::Unpushed {
            branch: u.branch.clone(),
            upstream: u.upstream.clone(),
            commits: u.commits,
        });
    }
    for g in &o.gone {
        work.push(Work::Gone(g.clone()));
    }
    if !o.stashes.is_empty() {
        work.push(Work::Stashes(o.stashes.clone()));
    }
    work
}

/// Every repository of `lists`, each once, in the order first met.
pub fn known_repos<'a>(lists: impl IntoIterator<Item = &'a [PathBuf]>) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = Vec::new();
    for path in lists.into_iter().flatten() {
        if !all.contains(path) {
            all.push(path.clone());
        }
    }
    all
}

/// "This computer", "WSL · Ubuntu", "SSH · build": the machine `repo` is on.
pub fn machine_of(repo: &Path) -> String {
    crate::remote::parse(repo).map_or_else(|| "This computer".to_owned(), |t| t.host.label())
}

/// Pushes `branch` of `repo`, asking nobody for a password: to its upstream when it has one,
/// else to a branch of the same name on `origin` (or the only remote), which becomes its
/// upstream.
pub fn push_branch(repo: &Path, branch: &str, has_upstream: bool) -> Result<(), String> {
    super::once_at_a_time("push", repo, || push_now(repo, branch, has_upstream))
}

fn push_now(repo: &Path, branch: &str, has_upstream: bool) -> Result<(), String> {
    let local = format!("refs/heads/{branch}");
    if has_upstream {
        let config = |key: &str| {
            git::run(
                Some(repo),
                &["config", "--get", &format!("branch.{branch}.{key}")],
            )
            .map(|s| s.trim().to_owned())
            .map_err(|_| format!("{branch} has no upstream set any more"))
        };
        let (remote, merge) = (config("remote")?, config("merge")?);
        if remote == "." {
            return Err(format!(
                "{branch} tracks a local branch: there is nothing to push it to"
            ));
        }
        let spec = format!("{local}:{merge}");
        return git::run_unattended_within(repo, &["push", "--", &remote, &spec], super::FETCH_FOR)
            .map(|_| ());
    }
    let remotes = git::remotes(repo)?;
    let remote = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or(match remotes.as_slice() {
            [only] => Some(only),
            _ => None,
        })
        .ok_or_else(|| {
            if remotes.is_empty() {
                "the repository has no remote to push to".to_owned()
            } else {
                "there is no origin, and several remotes: open the repository to choose one"
                    .to_owned()
            }
        })?;
    let spec = format!("{local}:{local}");
    git::run_unattended_within(
        repo,
        &["push", "--set-upstream", "--", &remote.name, &spec],
        super::FETCH_FOR,
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "my_work_tests.rs"]
mod tests;
