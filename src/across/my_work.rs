//! "Where's my work?": what exists in a repository and nowhere else (commits on no remote,
//! stashes, uncommitted changes here or in another worktree, branches whose remote branch is
//! gone), read from a repository's [`Overview`], and pushing a branch from there.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::git::{self, Overview, Tracks};

/// One piece of work only this repository has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Work {
    /// Files changed and not committed in the repository's own folder.
    Uncommitted(usize),
    /// The repository has commits and no remote: all of them are only here.
    NoRemote,
    /// A branch with commits its upstream (or, without one, any remote) doesn't have.
    Unpushed {
        branch: String,
        upstream: Option<String>,
        commits: u32,
        tracks: Tracks,
    },
    /// The stashes, by message, newest first.
    Stashes(Vec<String>),
    /// Another worktree of the repository with uncommitted files.
    Worktree { path: PathBuf, changes: usize },
    /// A branch whose remote branch was deleted, with commits no remote has.
    Gone { branch: String, commits: u32 },
}

impl Work {
    /// What the row says.
    pub fn describe(&self) -> String {
        match self {
            Work::Uncommitted(n) => format!("{} with uncommitted changes", files(*n)),
            Work::NoRemote => "no remote: its commits are only here".to_owned(),
            Work::Unpushed {
                branch,
                upstream: Some(up),
                commits,
                ..
            } => format!("{branch}: {} not pushed to {up}", commits_text(*commits)),
            Work::Unpushed {
                branch,
                commits,
                tracks: Tracks::Local,
                ..
            } => format!(
                "{branch}: {} on no remote (it tracks a branch of this repository)",
                commits_text(*commits)
            ),
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
            Work::Gone { branch, commits } => format!(
                "{branch}: its remote branch is gone, {} on no remote",
                commits_text(*commits)
            ),
        }
    }

    /// The icon of its row.
    pub fn icon(&self) -> &'static str {
        match self {
            Work::Uncommitted(_) | Work::Worktree { .. } => "file-diff",
            Work::Unpushed { .. } => "arrow-up",
            Work::Stashes(_) => "archive",
            Work::NoRemote | Work::Gone { .. } => "circle-alert",
        }
    }

    /// What Push sends: the branch, and whether it goes to its upstream (else to origin, which
    /// it then tracks). `None` when there is no Push for it, see [`Work::why_no_push`].
    pub fn push(&self) -> Option<(&str, bool)> {
        match self {
            Work::Unpushed {
                branch,
                tracks: Tracks::SameName,
                ..
            } => Some((branch, true)),
            Work::Unpushed {
                branch,
                tracks: Tracks::Nothing,
                ..
            } => Some((branch, false)),
            _ => None,
        }
    }

    /// Why a branch with commits to push has no Push: a push from here could only send it
    /// where it doesn't belong.
    pub fn why_no_push(&self) -> Option<String> {
        match self {
            Work::Unpushed {
                branch,
                upstream,
                tracks: Tracks::OtherName,
                ..
            } => Some(format!(
                "{branch} tracks {}, a branch of another name: open the repository to push it \
                 where it belongs",
                upstream.as_deref().unwrap_or("a branch of another name")
            )),
            Work::Unpushed {
                branch,
                tracks: Tracks::Local,
                ..
            } => Some(format!(
                "{branch} tracks a branch of this repository: open the repository to push it \
                 where it belongs"
            )),
            _ => None,
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
    if !o.has_remote && !o.head.is_empty() {
        work.push(Work::NoRemote);
    }
    for u in &o.unpushed {
        work.push(match u.tracks {
            // A gone branch with nothing on no remote isn't listed: merged and deleted there,
            // its commits are elsewhere.
            Tracks::Gone => Work::Gone {
                branch: u.branch.clone(),
                commits: u.commits,
            },
            tracks => Work::Unpushed {
                branch: u.branch.clone(),
                upstream: u.upstream.clone(),
                commits: u.commits,
                tracks,
            },
        });
    }
    if !o.stashes.is_empty() {
        work.push(Work::Stashes(o.stashes.clone()));
    }
    work
}

/// Every repository of `lists`, each once, in the order first met.
pub fn known_repos<'a>(lists: impl IntoIterator<Item = &'a [PathBuf]>) -> Vec<PathBuf> {
    let mut met: HashSet<&PathBuf> = HashSet::new();
    let mut all: Vec<PathBuf> = Vec::new();
    for path in lists.into_iter().flatten() {
        if met.insert(path) {
            all.push(path.clone());
        }
    }
    all
}

/// "This computer", "WSL · Ubuntu", "SSH · build": the machine `repo` is on.
pub fn machine_of(repo: &Path) -> String {
    crate::remote::parse(repo).map_or_else(|| "This computer".to_owned(), |t| t.host.label())
}

/// Pushes `branch` of `repo`, asking nobody for a password: to its upstream when it has one of
/// the same name, else to a branch of the same name on `origin` (or the only remote), which
/// becomes its upstream. Refuses whatever would send it elsewhere: an upstream of another
/// name, a push remote other than the one it tracks, a remote branch deleted there.
pub fn push_branch(repo: &Path, branch: &str, has_upstream: bool) -> Result<(), String> {
    super::once_at_a_time("push", repo, || push_now(repo, branch, has_upstream))
}

fn push_now(repo: &Path, branch: &str, has_upstream: bool) -> Result<(), String> {
    let config = |key: &str| {
        git::run(Some(repo), &["config", "--get", key])
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let local = format!("refs/heads/{branch}");
    // Read every time, whatever the look said: the push goes by what is set now.
    let tracked = config(&format!("branch.{branch}.remote"));
    let merge = config(&format!("branch.{branch}.merge"));
    if tracked.as_deref() == Some(".") {
        return Err(format!(
            "{branch} tracks a local branch: there is nothing to push it to"
        ));
    }
    let (remote, set_upstream) = match tracked {
        Some(remote) => {
            if let Some(merge) = merge.as_deref()
                && merge != local
            {
                let name = merge.strip_prefix("refs/heads/").unwrap_or(merge);
                return Err(format!(
                    "{branch} tracks {remote}/{name}, a branch of another name: open the \
                     repository to push it where it belongs"
                ));
            }
            let gone = git::run(
                Some(repo),
                &[
                    "rev-parse",
                    "--verify",
                    "-q",
                    &format!("refs/remotes/{remote}/{branch}"),
                ],
            )
            .is_err();
            if merge.is_some() && gone {
                return Err(format!(
                    "{branch}: its remote branch is gone (deleted there?): open the repository \
                     to push it again"
                ));
            }
            (remote, false)
        }
        None if has_upstream => return Err(format!("{branch} has no upstream set any more")),
        None => (default_remote(repo)?, true),
    };
    // Where a plain `git push` of it would go, when that is set to somewhere else.
    let push_to =
        config(&format!("branch.{branch}.pushRemote")).or_else(|| config("remote.pushDefault"));
    if let Some(other) = push_to
        && other != remote
    {
        return Err(format!(
            "{branch} is set to be pushed to {other}, not {remote}: open the repository to push \
             it"
        ));
    }
    let spec = format!("{local}:{local}");
    let mut args = vec!["push"];
    if set_upstream {
        args.push("--set-upstream");
    }
    args.extend(["--", remote.as_str(), spec.as_str()]);
    git::run_unattended_within(repo, &args, super::FETCH_FOR).map(|_| ())
}

/// `origin`, or the only remote.
fn default_remote(repo: &Path) -> Result<String, String> {
    let remotes = git::remotes(repo)?;
    remotes
        .iter()
        .find(|r| r.name == "origin")
        .or(match remotes.as_slice() {
            [only] => Some(only),
            _ => None,
        })
        .map(|r| r.name.clone())
        .ok_or_else(|| {
            if remotes.is_empty() {
                "the repository has no remote to push to".to_owned()
            } else {
                "there is no origin, and several remotes: open the repository to choose one"
                    .to_owned()
            }
        })
}

#[cfg(test)]
#[path = "my_work_tests.rs"]
mod tests;
