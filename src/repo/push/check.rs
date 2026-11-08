//! After a refused push: where it went, what the remote has now, and how the branch and the
//! remote branch stand to each other. Runs in the background; every git call goes through
//! [`crate::git`], so it works the same for a repository in WSL or over SSH.

use std::collections::HashSet;
use std::path::Path;

use crate::git;

/// How many of the commits only the remote has are named (in the banner and the confirmation).
pub(super) const LISTED: usize = 10;

/// How far back the branch's reflog is searched for the remote's commit (see `was_here`).
const REFLOG_DEPTH: &str = "1000";

/// A branch on a remote: where a push goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) remote: String,
    /// Its name there (`main`, not `refs/heads/main`).
    pub(crate) branch: String,
}

impl Target {
    /// `origin/main`.
    pub(crate) fn label(&self) -> String {
        format!("{}/{}", self.remote, self.branch)
    }
}

/// A commit only the remote has, as it is named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteCommit {
    pub(crate) short: String,
    pub(crate) author: String,
    pub(crate) subject: String,
}

/// How the branch and the remote branch stand, after a refused push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// The remote has nothing the branch doesn't (any more): a push goes through.
    Clear,
    /// The remote has commits, the branch none of its own: pull, there is nothing to push.
    Behind,
    /// Both have commits the other doesn't.
    Diverged,
    /// The branch here once was where the remote is now, and was rewritten (amended,
    /// rebased) or moved back (undone, reset) since: everything the remote has was here.
    Rewritten,
}

/// How a refused push stands, as checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rejection {
    /// The local branch (`main`).
    pub(crate) branch: String,
    /// Its commit when it was checked: what a force push pushes.
    pub(crate) local: String,
    pub(crate) target: Target,
    /// The remote branch's commit when it was checked: what a force push expects to replace.
    pub(crate) remote: String,
    /// Commits only the branch has, and only the remote has.
    pub(crate) ahead: u32,
    pub(crate) behind: u32,
    pub(crate) shape: Shape,
    /// The first [`LISTED`] commits only the remote has, newest first.
    pub(crate) missing: Vec<RemoteCommit>,
    /// The git there knows `--force-if-includes` (2.30 and later).
    pub(crate) if_includes: bool,
    /// The push was a first one (`-u`): what follows it sets the upstream too.
    pub(crate) publish: bool,
    /// A force push was refused because the remote moved since the last check.
    pub(crate) moved: bool,
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Checked {
    Rejected(Box<Rejection>),
    /// The remote branch is gone: a push publishes it again.
    Gone {
        branch: String,
        target: Target,
        /// A first push: pushing again publishes and tracks it (`-u`); otherwise the upstream
        /// is left as it is.
        publish: bool,
    },
}

/// What a check is asked about.
pub(super) struct Question {
    pub(super) branch: String,
    /// Where the push went, when the push said so itself (a publish, a force push).
    pub(super) target: Option<Target>,
    /// The ref on the remote the refused line names (`refs/heads/main`).
    pub(super) to: String,
    pub(super) remotes: Vec<String>,
    pub(super) publish: bool,
    pub(super) moved: bool,
}

/// Looks at the remote branch a refused push went to: fetches it, and counts.
pub(super) fn check(
    path: &Path,
    q: &Question,
    creds: Option<&git::Credentials>,
) -> Result<Checked, String> {
    let target = match &q.target {
        Some(t) => t.clone(),
        None => push_target(path, &q.branch, &q.to, &q.remotes)?,
    };
    let local = git::resolve(path, &format!("refs/heads/{}", q.branch))?;
    let dst = format!("refs/heads/{}", target.branch);
    // Where the push goes (`pushurl`, `pushInsteadOf`), not where fetches come from: that
    // repository is what the push was refused by.
    let url = git::run(
        Some(path),
        &["remote", "get-url", "--push", "--", &target.remote],
    )?;
    let url = url.trim();
    // What the remote has right now: the lease expects exactly this, and it is what is counted.
    let listed = git::run_with(Some(path), &["ls-remote", "--", url, &dst], creds)?;
    let Some(remote) = remote_commit(&listed, &dst) else {
        return Ok(Checked::Gone {
            branch: q.branch.clone(),
            target,
            publish: q.publish,
        });
    };
    git::run_with(
        Some(path),
        &["fetch", "--quiet", "--no-tags", "--", url, &dst],
        creds,
    )?;
    if git::resolve(path, &remote).is_err() {
        return Err(crate::i18n::message(
            "repo-workflows-push-check-67e345ac",
            &[("target-label-1", (target.label()).clone().into())],
        ));
    }
    let (ahead, behind) = apart(path, &local, &remote)?;
    let rewritten = behind > 0 && was_here(path, &q.branch, &remote);
    let missing = if behind > 0 {
        remote_only(path, &local, &remote)?
    } else {
        Vec::new()
    };
    let if_includes = git::run(Some(path), &["--version"])
        .ok()
        .and_then(|v| version(&v))
        .is_some_and(|v| v >= (2, 30));
    Ok(Checked::Rejected(Box::new(Rejection {
        branch: q.branch.clone(),
        local,
        target,
        remote,
        ahead,
        behind,
        shape: shape(ahead, behind, rewritten),
        missing,
        if_includes,
        publish: q.publish,
        moved: q.moved,
    })))
}

/// The remote a push of `branch` goes to, as git picks it (`branch.<b>.pushRemote`, then
/// `remote.pushDefault`, then `branch.<b>.remote`, then `origin`), with the branch there that
/// the refused line names (`to`).
fn push_target(path: &Path, branch: &str, to: &str, remotes: &[String]) -> Result<Target, String> {
    let config = |key: &str| {
        git::run_ok_codes(Some(path), &["config", "--get", key], &[0, 1])
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let remote = config(&format!("branch.{branch}.pushRemote"))
        .or_else(|| config("remote.pushDefault"))
        .or_else(|| config(&format!("branch.{branch}.remote")))
        .or_else(|| remotes.iter().find(|r| *r == "origin").cloned())
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-push-push-target-1b9119c5",
                &[("branch", (branch).clone().into())],
            )
        })?;
    pick_target(&remote, branch, to, remotes)
}

/// The target for `remote` and the refused line's `to`: the remote must be one of `remotes`
/// (not `.`, a branch of this repository, nor a URL).
pub(super) fn pick_target(
    remote: &str,
    branch: &str,
    to: &str,
    remotes: &[String],
) -> Result<Target, String> {
    if !remotes.iter().any(|r| r == remote) {
        return Err(crate::i18n::message(
            "repo-workflows-push-pick-target-acf19d61",
            &[
                ("branch", (branch).clone().into()),
                ("remote", (remote).clone().into()),
            ],
        ));
    }
    let name = to.strip_prefix("refs/heads/").unwrap_or(branch);
    Ok(Target {
        remote: remote.to_owned(),
        branch: name.to_owned(),
    })
}

/// The commit `ls-remote` gives for exactly `dst`.
pub(super) fn remote_commit(listed: &str, dst: &str) -> Option<String> {
    listed.lines().find_map(|line| {
        let (sha, name) = line.split_once('\t')?;
        (name.trim() == dst && !sha.is_empty()).then(|| sha.to_owned())
    })
}

/// Commits only `local` has, and only `remote` has.
fn apart(path: &Path, local: &str, remote: &str) -> Result<(u32, u32), String> {
    let out = git::run(
        Some(path),
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{local}...{remote}"),
        ],
    )?;
    parse_counts(&out).ok_or_else(|| {
        crate::i18n::message(
            "repo-workflows-push-apart-b578bb02",
            &[("out", (out).clone().into())],
        )
    })
}

/// `3\t5` → (3, 5).
pub(super) fn parse_counts(out: &str) -> Option<(u32, u32)> {
    let mut parts = out.split_whitespace();
    let ahead = parts.next()?.parse().ok()?;
    let behind = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// The commits only `remote` has, newest first, the first [`LISTED`].
fn remote_only(path: &Path, local: &str, remote: &str) -> Result<Vec<RemoteCommit>, String> {
    let out = git::run(
        Some(path),
        &[
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            &format!("-n{LISTED}"),
            "--format=%h%x1f%an%x1f%s",
            &format!("{local}..{remote}"),
        ],
    )?;
    Ok(parse_commits(&out))
}

pub(super) fn parse_commits(out: &str) -> Vec<RemoteCommit> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.splitn(3, '\x1f');
            Some(RemoteCommit {
                short: f.next()?.to_owned(),
                author: f.next()?.to_owned(),
                subject: f.next()?.to_owned(),
            })
        })
        .collect()
}

/// Whether `branch` was once (by its reflog) at the remote's commit itself: then everything
/// the remote has was here, and the branch was rewritten (amended, rebased, reset) after it was
/// pushed. A commit someone else pushed on top was never here, and says no, as does a missing
/// reflog (turned off, expired).
fn was_here(path: &Path, branch: &str, remote: &str) -> bool {
    git::run(
        Some(path),
        &[
            "log",
            "-g",
            "--no-show-signature",
            "-n",
            REFLOG_DEPTH,
            "--format=%H",
            &format!("refs/heads/{branch}"),
            "--",
        ],
    )
    .is_ok_and(|reflog| any_in(&reflog, remote))
}

/// Whether a hash listed in `b` is listed in `a` (one per line).
pub(super) fn any_in(a: &str, b: &str) -> bool {
    let seen: HashSet<&str> = a.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    b.lines().map(str::trim).any(|l| seen.contains(l))
}

pub(super) fn shape(ahead: u32, behind: u32, rewritten: bool) -> Shape {
    match (ahead, behind) {
        (_, 0) => Shape::Clear,
        _ if rewritten => Shape::Rewritten,
        (0, _) => Shape::Behind,
        _ => Shape::Diverged,
    }
}

/// `git version 2.45.1.windows.1` → (2, 45).
pub(super) fn version(out: &str) -> Option<(u32, u32)> {
    let number = out
        .split_whitespace()
        .find(|w| w.starts_with(char::is_numeric))?;
    let mut parts = number.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_whatever_follows_them() {
        assert_eq!(version("git version 2.43.0\n"), Some((2, 43)));
        assert_eq!(version("git version 2.45.1.windows.1"), Some((2, 45)));
        assert_eq!(version("git version 2.39.3 (Apple Git-146)"), Some((2, 39)));
        assert_eq!(version("git version 2.30.0-rc1"), Some((2, 30)));
        assert_eq!(version("not git"), None);
        assert!(version("git version 2.29.2").is_some_and(|v| v < (2, 30)));
    }

    #[test]
    fn the_shape_follows_the_counts() {
        assert_eq!(shape(2, 0, false), Shape::Clear);
        assert_eq!(shape(0, 0, false), Shape::Clear);
        assert_eq!(shape(0, 3, false), Shape::Behind);
        // Moved back (an undone commit): what the remote has was here.
        assert_eq!(shape(0, 3, true), Shape::Rewritten);
        assert_eq!(shape(2, 3, false), Shape::Diverged);
        assert_eq!(shape(2, 3, true), Shape::Rewritten);
    }

    #[test]
    fn counts_and_commits_are_parsed() {
        assert_eq!(parse_counts("3\t5\n"), Some((3, 5)));
        assert_eq!(parse_counts("3"), None);
        assert_eq!(parse_counts("x\t1"), None);
        let commits = parse_commits("abc1234\x1fAnn\x1fFix: a\x1fb\nbad line\n");
        assert_eq!(
            commits,
            vec![RemoteCommit {
                short: "abc1234".to_owned(),
                author: "Ann".to_owned(),
                subject: "Fix: a\x1fb".to_owned(),
            }]
        );
    }

    #[test]
    fn ls_remote_is_read_for_exactly_the_branch() {
        let listed = "1111\trefs/heads/x/refs/heads/main\n2222\trefs/heads/main\n";
        assert_eq!(
            remote_commit(listed, "refs/heads/main"),
            Some("2222".into())
        );
        assert_eq!(remote_commit("", "refs/heads/main"), None);
    }

    #[test]
    fn a_hash_in_both_lists_is_found() {
        assert!(any_in("a\nb\nc\n", "x\nb\n"));
        assert!(!any_in("a\nb\n", "x\ny\n"));
        assert!(!any_in("", "x"));
        assert!(!any_in("a\n\n", "\n"));
    }

    #[test]
    fn the_target_is_a_known_remote() {
        let remotes = vec!["origin".to_owned(), "fork".to_owned()];
        let t = pick_target("fork", "dev", "refs/heads/feature", &remotes);
        assert_eq!(
            t,
            Ok(Target {
                remote: "fork".into(),
                branch: "feature".into()
            })
        );
        assert!(pick_target(".", "dev", "refs/heads/dev", &remotes).is_err());
        assert!(pick_target("https://x/y.git", "dev", "refs/heads/dev", &remotes).is_err());
        // A line that names no branch: the branch's own name.
        assert_eq!(
            pick_target("origin", "dev", "", &remotes).map(|t| t.branch),
            Ok("dev".to_owned())
        );
    }
}
