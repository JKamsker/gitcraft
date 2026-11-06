//! Starting work on an issue in real repositories, through the steps the tab runs: the branch or
//! worktree made, named after the issue, and the link to the issue kept and read back.

use std::path::{Path, PathBuf};

use super::link::{self, IssueLink, Place, Start};
use super::naming::branch_name;
use crate::forge::Issue;
use crate::git;
use crate::repo::OpKind;
use crate::repo::operations::run_steps;

/// A repository with a commit on `main`, and `origin/main` a commit ahead of it (the remote
/// has moved on since the last pull), in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-issue-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(worktree_dir(&dir));
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["symbolic-ref", "HEAD", "refs/heads/main"],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["commit", "-q", "--allow-empty", "-m", "on the remote"],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &["reset", "-q", "--hard", "HEAD~1"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

fn worktree_dir(repo: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wt", repo.display()))
}

fn clean(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(worktree_dir(dir));
}

fn issue(number: u64, title: &str) -> Issue {
    Issue {
        number,
        title: title.into(),
        body:
            "Login crashes.\n\n## Acceptance criteria\n- [ ] no crash\n- [x] a test\n\n## Notes\n\
               later"
                .into(),
        url: format!("https://github.com/o/r/issues/{number}"),
        labels: vec!["bug".into()],
    }
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    git::run(Some(dir), args)
        .map(|out| out.trim().to_owned())
        .unwrap_or_default()
}

#[test]
fn a_branch_named_after_the_issue_is_made_and_checked_out_with_its_link() {
    let dir = repo("branch");
    let issue = issue(42, "Fix login crash");
    let link = IssueLink::of(&issue);
    let start = Start {
        branch: branch_name(issue.number, &issue.title),
        from: "refs/remotes/origin/main".into(),
        exists: false,
        place: Place::Here,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let at = git_out(&dir, &["rev-parse", "HEAD"]);
    let remote = git_out(&dir, &["rev-parse", "origin/main"]);
    let tracks = git::run(Some(&dir), &["config", "branch.42-fix-login-crash.merge"]);
    let links = link::read(&dir);
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(current, "42-fix-login-crash");
    // From where the remote's default branch is, and not tracking it.
    assert_eq!(at, remote);
    assert!(tracks.is_err(), "{tracks:?}");
    let links = links.unwrap_or_default();
    let read = links.get("42-fix-login-crash");
    assert_eq!(read, Some(&link));
    assert_eq!(
        read.map(|l| l.criteria.as_str()),
        Some("- [ ] no crash\n- [x] a test")
    );
}

#[test]
fn a_worktree_named_after_the_issue_shares_the_link() {
    let dir = repo("worktree");
    let issue = issue(7, "Füße — ünïcode 日本語 🎉");
    let link = IssueLink::of(&issue);
    let name = branch_name(issue.number, &issue.title);
    let folder = worktree_dir(&dir);
    let start = Start {
        branch: name.clone(),
        from: "refs/remotes/origin/main".into(),
        exists: false,
        place: Place::NewWorktree(folder.to_string_lossy().into_owned()),
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let here = git_out(&dir, &["branch", "--show-current"]);
    let there = git_out(&folder, &["branch", "--show-current"]);
    // Read from the worktree, as its own tab reads them.
    let links = link::read(&folder).unwrap_or_default();
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(name, "7-füße-ünïcode-日本語");
    assert_eq!(here, "main");
    assert_eq!(there, name);
    assert_eq!(links.get(&name), Some(&link));
}

#[test]
fn a_branch_there_already_is_checked_out_as_it_is() {
    let dir = repo("exists");
    assert!(git::run(Some(&dir), &["branch", "3-again"]).is_ok());
    let before = git_out(&dir, &["rev-parse", "3-again"]);
    let link = IssueLink::of(&issue(3, "Again"));
    let start = Start {
        branch: "3-again".into(),
        from: "refs/remotes/origin/main".into(),
        exists: true,
        place: Place::Here,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let after = git_out(&dir, &["rev-parse", "HEAD"]);
    let links = link::read(&dir).unwrap_or_default();
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!((current.as_str(), after), ("3-again", before));
    assert_eq!(links.get("3-again").map(|l| l.number), Some(3));
}

#[test]
fn the_link_follows_the_branch_and_can_be_forgotten() {
    let dir = repo("follow");
    let link = IssueLink {
        number: 9,
        title: "-starts like an option".into(),
        url: String::new(),
        criteria: "- [ ] a\n\n  - [ ] nested: yes".into(),
    };
    let steps = [
        &["branch", "9-x"][..],
        &["branch", "release.v1.2"],
        &["branch", "other"],
    ];
    for args in steps {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let mut ops = link::write_ops("9-x", &link);
    ops.extend(link::write_ops("release.v1.2", &link));
    let (wrote, _) = run_steps(&dir, &ops, OpKind::plain(), None);
    let written = link::read(&dir).unwrap_or_default();
    let _ = git::run(Some(&dir), &["branch", "-m", "9-x", "9-renamed"]);
    let renamed = link::read(&dir).unwrap_or_default();
    let (unlinked, _) = run_steps(&dir, &link::unlink_ops("9-renamed"), OpKind::plain(), None);
    let _ = git::run(Some(&dir), &["branch", "-D", "release.v1.2"]);
    let after = link::read(&dir);
    clean(&dir);
    assert!(wrote.is_ok(), "{wrote:?}");
    assert_eq!(written.get("9-x"), Some(&link));
    assert_eq!(written.get("release.v1.2"), Some(&link));
    assert!(!written.contains_key("other"));
    assert_eq!(renamed.get("9-renamed"), Some(&link));
    assert!(!renamed.contains_key("9-x"));
    assert!(unlinked.is_ok(), "{unlinked:?}");
    // No branch has one any more: git says so with exit code 1, which is no error.
    assert_eq!(after.map(|l| l.len()), Ok(0));
}
