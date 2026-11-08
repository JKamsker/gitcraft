//! A link to a forge's page, made sense of here: the clone of its repository among those
//! gitcraft knows (open tabs, recent repositories), by their remotes' URLs, and what to show
//! there. Off the window's thread: it asks git.

use std::path::PathBuf;

use super::link::{ForgeLink, Target};
use super::resolve::{Resolved, Then};
use crate::git;

/// A repository a link's clone may be: its folder, and its remotes when they are already
/// known (an open tab's), else read when looked at.
#[derive(Clone, Debug)]
pub struct Known {
    pub path: PathBuf,
    pub remotes: Option<Vec<git::Remote>>,
}

/// How a clone has a link's repository.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Has {
    /// As its hosting remote (`origin`, else the first): its pull requests are the link's.
    Hosting,
    /// As another remote (a fork's `upstream`).
    Other,
}

/// Where `link` opens: in the first of `clones` whose hosting remote is its repository, else
/// in one with it as another remote, or an offer to clone it when none has it.
pub(crate) fn resolve_link(link: &ForgeLink, clones: &[Known]) -> Result<Resolved, String> {
    let mut found: Option<(Has, &Known, String)> = None;
    for clone in clones {
        let Some((has, remote)) = has_repo(clone, link) else {
            continue;
        };
        if found.as_ref().is_none_or(|(best, _, _)| has < *best) {
            found = Some((has, clone, remote));
        }
        if has == Has::Hosting {
            break;
        }
    }
    let Some((_, clone, remote)) = found else {
        return Ok(Resolved {
            root: PathBuf::new(),
            then: Then::Clone(link.clone_url()),
        });
    };
    let root = &clone.path;
    let then = match &link.target {
        Target::Repo => Then::Nothing,
        Target::PullRequest(number) => Then::PullRequest {
            number: *number,
            remote,
        },
        Target::Commit(sha) => Then::Commit(git::resolve(root, sha).map_err(|_| {
            format!(
                "Commit {sha} isn't in {}: fetch, then open the link again",
                root.display()
            )
        })?),
        Target::File { rev, path, line } => file_in_clone(root, rev, path, *line)?,
    };
    Ok(Resolved {
        root: root.clone(),
        then,
    })
}

/// Whether, and how, `clone` has `link`'s repository among its remotes (a clone that can't be
/// read hasn't).
fn has_repo(clone: &Known, link: &ForgeLink) -> Option<(Has, String)> {
    let read;
    let remotes = match &clone.remotes {
        Some(remotes) => remotes,
        None => {
            read = git::remotes(&clone.path).ok()?;
            &read
        }
    };
    let hosting = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| remotes.first())?;
    if link.is_remote(&hosting.url) {
        Some((Has::Hosting, hosting.name.clone()))
    } else {
        remotes
            .iter()
            .find(|r| link.is_remote(&r.url))
            .map(|r| (Has::Other, r.name.clone()))
    }
}

/// The file a link names, in `root`. A link's path can't tell a branch `feature/x` from a
/// folder `x`: each way of splitting it is tried, the branch's first, then the file as the
/// clone has it (the linked branch not fetched). The linked commit is used when the clone has
/// it.
fn file_in_clone(
    root: &std::path::Path,
    rev: &str,
    path: &str,
    line: Option<u32>,
) -> Result<Then, String> {
    let whole = format!("{rev}/{path}");
    let names: Vec<&str> = whole.split('/').collect();
    let splits: Vec<(String, String)> = (1..names.len())
        .map(|i| (names[..i].join("/"), names[i..].join("/")))
        .collect();
    let has = |commit: &str, path: &str| {
        git::run(Some(root), &["cat-file", "-e", &format!("{commit}:{path}")]).is_ok()
    };
    for (rev, path) in &splits {
        if let Ok(hash) = git::resolve(root, rev)
            && has(&hash, path)
        {
            return Ok(Then::FileAt {
                rev: Some(hash),
                path: path.clone(),
                line,
            });
        }
    }
    for (_, path) in &splits {
        let in_worktree = !crate::remote::is_remote(root) && root.join(path).is_file();
        if has("HEAD", path) || in_worktree {
            return Ok(Then::FileAt {
                rev: None,
                path: path.clone(),
                line,
            });
        }
    }
    Err(format!(
        "{path} isn't in {}: fetch, then open the link again",
        root.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::link::parse_web_url;

    fn repo(name: &str, remote: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-link {name} ü-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("src/a b.rs"), "x\n").map_err(|e| e.to_string())?;
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["remote", "add", "origin", remote],
            &["add", "-A"],
            &["commit", "-q", "-m", "one"],
            &["branch", "feature/x"],
        ] {
            git::run(Some(&dir), args)?;
        }
        Ok(dir)
    }

    /// Among two clones, the one whose remote is the link's; and on it the link's commit,
    /// pull request or file.
    #[test]
    fn a_link_opens_in_its_clone() -> Result<(), String> {
        let other = repo("other", "https://github.com/o/other.git")?;
        let mine = repo("mine", "git@github.com:O/R.git")?;
        let missing = std::env::temp_dir().join("gitcraft-link-no-such-folder");
        let fork = repo("fork", "https://github.com/me/r.git")?;
        git::run(
            Some(&fork),
            &["remote", "add", "upstream", "https://github.com/o/r"],
        )?;
        let known = |path: &PathBuf| Known {
            path: path.clone(),
            remotes: None,
        };
        // The fork comes first, as an open tab would; its remotes known already.
        let fork_known = Known {
            path: fork.clone(),
            remotes: git::remotes(&fork).ok(),
        };
        let clones = [
            known(&missing),
            fork_known.clone(),
            known(&other),
            known(&mine),
        ];
        let only_fork = [fork_known];
        let head = git::run(Some(&mine), &["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        let short = head.get(..10).unwrap_or(&head).to_owned();
        let open =
            |url: &str| -> Result<Resolved, String> { resolve_link(&parse_web_url(url)?, &clones) };
        let pr = open("https://github.com/o/r/pull/12");
        let commit = open(&format!("https://github.com/o/r/commit/{short}"));
        let file = open("https://github.com/o/r/blob/main/src/a%20b.rs#L1");
        // A branch with a `/`: told from the folders by what the clone has.
        let slashed = open("https://github.com/o/r/blob/feature/x/src/a%20b.rs");
        // A branch the clone doesn't have: the file as the clone has it.
        let unfetched = open("https://github.com/o/r/blob/elsewhere/src/a%20b.rs#L1");
        let no_file = open("https://github.com/o/r/blob/main/src/gone.rs");
        let unknown = open("https://github.com/o/r/commit/deadbeefdeadbeef");
        let none = open("https://codeberg.org/someone/else/pulls/1");
        let fork_pr = resolve_link(&parse_web_url("https://github.com/o/r/pull/3")?, &only_fork);
        let fork_head = git::run(Some(&fork), &["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        let fork_commit = resolve_link(
            &parse_web_url(&format!("https://github.com/o/r/commit/{fork_head}"))?,
            &only_fork,
        );
        let _ = std::fs::remove_dir_all(&other);
        let _ = std::fs::remove_dir_all(&fork);
        let _ = std::fs::remove_dir_all(&mine);
        // The clone whose origin is o/r, not the fork listed before it.
        let pr = pr?;
        assert_eq!(pr.root, mine);
        let fork_pr = fork_pr?;
        assert_eq!(fork_pr.root, fork);
        assert_eq!(
            fork_pr.then,
            Then::PullRequest {
                number: 3,
                remote: "upstream".into(),
            }
        );
        // A commit is also opened in the fork.
        assert_eq!(fork_commit?.then, Then::Commit(fork_head));
        assert_eq!(
            pr.then,
            Then::PullRequest {
                number: 12,
                remote: "origin".into()
            }
        );
        assert_eq!(commit?.then, Then::Commit(head.clone()));
        assert_eq!(
            file?.then,
            Then::FileAt {
                rev: Some(head.clone()),
                path: "src/a b.rs".into(),
                line: Some(1)
            }
        );
        assert_eq!(
            slashed?.then,
            Then::FileAt {
                rev: Some(head.clone()),
                path: "src/a b.rs".into(),
                line: None
            }
        );
        assert_eq!(
            unfetched?.then,
            Then::FileAt {
                rev: None,
                path: "src/a b.rs".into(),
                line: Some(1)
            }
        );
        assert!(
            no_file
                .err()
                .is_some_and(|e| e.contains("src/gone.rs isn't in"))
        );
        assert!(
            unknown
                .err()
                .is_some_and(|e| e.contains("fetch, then open the link again"))
        );
        assert_eq!(
            none?.then,
            Then::Clone("https://codeberg.org/someone/else.git".into())
        );
        Ok(())
    }
}
