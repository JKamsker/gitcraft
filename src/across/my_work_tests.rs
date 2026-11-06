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
fn pushes_a_branch_to_its_upstream_even_under_another_name() -> Result<(), String> {
    let (repo, origin) = pushed_repo("push-upstream")?;
    run(&repo, &["checkout", "-q", "-b", "mine"])?;
    commit(&repo, "two")?;
    run(&repo, &["push", "-q", "origin", "mine:theirs-name"])?;
    run(
        &repo,
        &["branch", "-q", "--set-upstream-to", "origin/theirs-name"],
    )?;
    commit(&repo, "three")?;
    // Checked out elsewhere: the push is by name, not of HEAD.
    run(&repo, &["checkout", "-q", "main"])?;
    let before = look(&repo)?;
    assert!(work_of(&before).iter().any(|w| matches!(w,
        Work::Unpushed { branch, upstream: Some(up), commits: 1 }
            if branch == "mine" && up == "origin/theirs-name")));
    push_branch(&repo, "mine", true)?;
    let mine = run(&repo, &["rev-parse", "mine"])?;
    let theirs = run(&origin, &["rev-parse", "refs/heads/theirs-name"])?;
    assert_eq!(mine, theirs);
    assert!(run(&origin, &["rev-parse", "--verify", "-q", "refs/heads/mine"]).is_err());
    run(&repo, &["fetch", "-q"])?;
    assert!(work_of(&look(&repo)?).is_empty());
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
        Work::Gone("old".into()).describe(),
        "old: its remote branch is gone"
    );
    assert_eq!(Work::Uncommitted(3).icon(), "file-diff");
}
