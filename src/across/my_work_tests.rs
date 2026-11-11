//! Where's my work? against real repositories: what is listed, and pushing from the list.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-mywork-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A bare `origin.git` and a clone of it, `repo`, with one commit pushed to `main`.
fn pushed_repo(name: &str) -> Result<(PathBuf, PathBuf), String> {
    let root = scratch(name)?;
    run(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"])?;
    let repo = root.join("my repo ✓");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "T"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["remote", "add", "origin", "../origin.git"],
    ] {
        run(&repo, args)?;
    }
    commit(&repo, "one")?;
    run(&repo, &["push", "-q", "-u", "origin", "main"])?;
    Ok((repo, root.join("origin.git")))
}

fn commit(repo: &Path, text: &str) -> Result<(), String> {
    std::fs::write(repo.join("f.txt"), text).map_err(|e| e.to_string())?;
    run(repo, &["add", "-A"])?;
    run(repo, &["commit", "-qm", text]).map(|_| ())
}

fn look(repo: &Path) -> Result<Overview, String> {
    git::overview(repo, Duration::from_secs(60))
}

#[test]
fn lists_the_work_most_at_risk_first() -> Result<(), String> {
    let (repo, _) = pushed_repo("list")?;
    commit(&repo, "two")?;
    run(&repo, &["branch", "local-only"])?;
    std::fs::write(repo.join("f.txt"), "stash me").map_err(|e| e.to_string())?;
    run(&repo, &["stash", "push", "-q", "-m", "wip ü"])?;
    std::fs::write(repo.join("new.txt"), "x").map_err(|e| e.to_string())?;
    let work = work_of(&look(&repo)?);
    let texts: Vec<String> = work.iter().map(Work::describe).collect();
    assert_eq!(
        texts,
        [
            "1 file with uncommitted changes",
            "main: 1 commit not pushed to origin/main",
            "local-only: 1 commit on no remote",
            "1 stash: wip ü",
        ]
    );
    Ok(())
}

#[test]
fn a_repository_with_nothing_only_here_lists_nothing() -> Result<(), String> {
    let (repo, _) = pushed_repo("nothing")?;
    assert!(work_of(&look(&repo)?).is_empty());
    Ok(())
}

#[test]
fn pushes_a_branch_to_its_upstream_by_name() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-upstream")?;
    run(&repo, &["checkout", "-q", "-b", "mine"])?;
    commit(&repo, "two")?;
    run(&repo, &["push", "-q", "-u", "origin", "mine"])?;
    commit(&repo, "three")?;
    // Checked out elsewhere: the push is by name, not of HEAD.
    run(&repo, &["checkout", "-q", "main"])?;
    let work = work_of(&look(&repo)?);
    assert!(work.iter().any(|w| matches!(w,
        Work::Unpushed { branch, upstream: Some(up), commits: 1, tracks: Tracks::SameName }
            if branch == "mine" && up == "origin/mine")));
    let pushes: Vec<_> = work.iter().filter_map(Work::push).collect();
    assert_eq!(pushes, [("mine", true)]);
    push_branch(&repo, "mine", true)?;
    assert_eq!(
        run(&repo, &["rev-parse", "mine"])?,
        run(&origin, &["rev-parse", "refs/heads/mine"])?
    );
    run(&repo, &["fetch", "-q"])?;
    assert!(work_of(&look(&repo)?).is_empty());
    Ok(())
}

/// `git checkout -b feature origin/main` makes `feature` track `origin/main`: pushing it there
/// would put its commits on main. There is no Push for it, and a push is refused.
#[test]
fn never_pushes_onto_a_branch_of_another_name() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-other-name")?;
    run(&repo, &["checkout", "-q", "-b", "feature", "origin/main"])?;
    commit(&repo, "two")?;
    let work = work_of(&look(&repo)?);
    let row = work
        .iter()
        .find(|w| matches!(w, Work::Unpushed { branch, .. } if branch == "feature"))
        .ok_or("feature isn't listed")?;
    assert_eq!(
        row.describe(),
        "feature: 1 commit not pushed to origin/main"
    );
    assert_eq!(row.push(), None);
    assert!(
        row.why_no_push()
            .is_some_and(|why| why.contains("another name")),
        "{row:?}"
    );
    let before = run(&origin, &["rev-parse", "refs/heads/main"])?;
    let err = push_branch(&repo, "feature", true)
        .err()
        .unwrap_or_default();
    assert!(
        err.contains("origin/main, a branch of another name"),
        "{err}"
    );
    let err = push_branch(&repo, "feature", false)
        .err()
        .unwrap_or_default();
    assert!(err.contains("another name"), "{err}");
    assert_eq!(run(&origin, &["rev-parse", "refs/heads/main"])?, before);
    assert!(
        run(
            &origin,
            &["rev-parse", "--verify", "-q", "refs/heads/feature"]
        )
        .is_err()
    );
    Ok(())
}

/// A push remote set elsewhere (a fork workflow) means the push doesn't belong on the remote
/// the branch tracks.
#[test]
fn refuses_a_push_set_to_go_elsewhere() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-remote")?;
    run(&repo, &["remote", "add", "fork", "../origin.git"])?;
    commit(&repo, "two")?;
    run(&repo, &["config", "branch.main.pushRemote", "fork"])?;
    let err = push_branch(&repo, "main", true).err().unwrap_or_default();
    assert!(err.contains("pushed to fork"), "{err}");
    run(&repo, &["config", "--unset", "branch.main.pushRemote"])?;
    run(&repo, &["config", "remote.pushDefault", "fork"])?;
    let err = push_branch(&repo, "main", true).err().unwrap_or_default();
    assert!(err.contains("pushed to fork"), "{err}");
    assert_ne!(
        run(&repo, &["rev-parse", "main"])?,
        run(&origin, &["rev-parse", "refs/heads/main"])?
    );
    Ok(())
}

/// A branch tracking a branch of this repository has no Push, and a push leaves its tracking
/// as it was.
#[test]
fn a_branch_tracking_a_local_one_isnt_pushed() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-local")?;
    run(
        &repo,
        &["checkout", "-q", "-b", "follower", "--track", "main"],
    )?;
    commit(&repo, "two")?;
    let work = work_of(&look(&repo)?);
    let row = work
        .iter()
        .find(|w| matches!(w, Work::Unpushed { branch, .. } if branch == "follower"))
        .ok_or("follower isn't listed")?;
    assert!(matches!(
        row,
        Work::Unpushed {
            tracks: Tracks::Local,
            ..
        }
    ));
    assert_eq!(row.push(), None);
    assert!(
        row.describe()
            .contains("tracks a branch of this repository")
    );
    let err = push_branch(&repo, "follower", false)
        .err()
        .unwrap_or_default();
    assert!(err.contains("local branch"), "{err}");
    assert_eq!(
        run(&repo, &["config", "branch.follower.remote"])?.trim(),
        "."
    );
    assert!(
        run(
            &origin,
            &["rev-parse", "--verify", "-q", "refs/heads/follower"]
        )
        .is_err()
    );
    Ok(())
}

/// A repository with commits and no remote has nothing anywhere else.
#[test]
fn a_repository_with_no_remote_isnt_safe() -> Result<(), String> {
    let root = scratch("no-remote")?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "T"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        run(&root, args)?;
    }
    // Before the first commit there is nothing to lose.
    assert!(work_of(&look(&root)?).is_empty());
    commit(&root, "one")?;
    let work = work_of(&look(&root)?);
    assert_eq!(work, vec![Work::NoRemote]);
    assert_eq!(work[0].describe(), "no remote: its commits are only here");
    assert_eq!(work[0].push(), None);
    Ok(())
}

/// A branch whose remote branch was deleted is one row with its count, and only while it has
/// commits on no remote: one merged and deleted there has nothing only here.
#[test]
fn a_gone_branch_is_one_row_and_only_with_its_own_commits() -> Result<(), String> {
    let (repo, origin) = pushed_repo("gone")?;
    for b in ["merged", "kept"] {
        run(&repo, &["checkout", "-q", "-b", b, "main"])?;
        commit(&repo, b)?;
        run(&repo, &["push", "-q", "-u", "origin", b])?;
    }
    commit(&repo, "kept two")?;
    run(&repo, &["checkout", "-q", "main"])?;
    run(&repo, &["merge", "-q", "--ff-only", "merged"])?;
    run(&repo, &["push", "-q", "origin", "main"])?;
    for b in ["merged", "kept"] {
        run(&origin, &["branch", "-q", "-D", b])?;
    }
    run(&repo, &["fetch", "-q", "--prune"])?;
    let work = work_of(&look(&repo)?);
    let texts: Vec<String> = work.iter().map(Work::describe).collect();
    assert_eq!(
        texts,
        ["kept: its remote branch is gone, 2 commits on no remote"]
    );
    assert_eq!(work[0].push(), None);
    // And a push of it from elsewhere doesn't make it again.
    let err = push_branch(&repo, "kept", true).err().unwrap_or_default();
    assert!(err.contains("gone"), "{err}");
    assert!(run(&origin, &["rev-parse", "--verify", "-q", "refs/heads/kept"]).is_err());
    Ok(())
}

#[test]
fn pushes_a_new_branch_to_origin_and_tracks_it() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-new")?;
    run(&repo, &["checkout", "-q", "-b", "feature/ünï"])?;
    commit(&repo, "two")?;
    push_branch(&repo, "feature/ünï", false)?;
    assert_eq!(
        run(&repo, &["rev-parse", "HEAD"])?,
        run(&origin, &["rev-parse", "refs/heads/feature/ünï"])?
    );
    let up = run(
        &repo,
        &["rev-parse", "--abbrev-ref", "feature/ünï@{upstream}"],
    )?;
    assert_eq!(up.trim(), "origin/feature/ünï");
    Ok(())
}

#[test]
fn says_why_a_push_cant_be_made() -> Result<(), String> {
    let (repo, _) = pushed_repo("push-refused")?;
    run(&repo, &["remote", "rename", "origin", "a"])?;
    run(&repo, &["remote", "add", "b", "../origin.git"])?;
    run(&repo, &["branch", "x"])?;
    let err = push_branch(&repo, "x", false).err().unwrap_or_default();
    assert!(err.contains("several remotes"), "{err}");
    run(&repo, &["remote", "remove", "a"])?;
    run(&repo, &["remote", "remove", "b"])?;
    let err = push_branch(&repo, "x", false).err().unwrap_or_default();
    assert!(err.contains("no remote"), "{err}");
    let err = push_branch(&repo, "x", true).err().unwrap_or_default();
    assert!(err.contains("no upstream"), "{err}");
    // Tracking a branch of this repository: nothing to push to.
    run(&repo, &["branch", "-q", "--set-upstream-to", "main", "x"])?;
    let err = push_branch(&repo, "x", true).err().unwrap_or_default();
    assert!(err.contains("local branch"), "{err}");
    Ok(())
}

#[test]
fn a_push_the_remote_refuses_says_git_s_reason() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-behind")?;
    // Someone else pushed meanwhile: ours is behind and ahead.
    let other = origin.with_file_name("other");
    run(
        origin.parent().ok_or("no parent")?,
        &["clone", "-q", "origin.git", "other"],
    )?;
    for args in [
        &["config", "user.name", "O"][..],
        &["config", "user.email", "o@o"],
        &["config", "commit.gpgSign", "false"],
    ] {
        run(&other, args)?;
    }
    commit(&other, "theirs")?;
    run(&other, &["push", "-q", "origin", "main"])?;
    commit(&repo, "ours")?;
    let err = push_branch(&repo, "main", true).err().unwrap_or_default();
    assert!(err.contains("rejected"), "{err}");
    Ok(())
}

#[test]
fn known_repositories_are_listed_once_in_order() {
    let a = PathBuf::from("/a");
    let b = PathBuf::from("ssh://h/b");
    let c = PathBuf::from(r"\\wsl.localhost\Ubuntu\c");
    let recent = vec![a.clone(), b.clone()];
    let tabs = vec![b.clone(), c.clone()];
    let none: Vec<PathBuf> = Vec::new();
    assert_eq!(
        known_repos([recent.as_slice(), tabs.as_slice(), none.as_slice()]),
        vec![a, b, c]
    );
}

#[test]
fn machines_are_named() {
    assert_eq!(machine_of(Path::new("/home/me/x")), "This computer");
    assert_eq!(machine_of(Path::new("ssh://build/x")), "SSH · build");
}

#[test]
fn rows_read_in_words() {
    let many = Work::Stashes(vec!["latest".into(), "older".into()]);
    assert_eq!(many.describe(), "2 stashes, the latest: latest");
    let wt = Work::Worktree {
        path: PathBuf::from("/w/side"),
        changes: 1,
    };
    assert_eq!(
        wt.describe(),
        "worktree /w/side: 1 file with uncommitted changes"
    );
    assert_eq!(
        Work::Gone {
            branch: "old".into(),
            commits: 1
        }
        .describe(),
        "old: its remote branch is gone, 1 commit on no remote"
    );
    assert_eq!(Work::Uncommitted(3).icon(), "file-diff");
}
