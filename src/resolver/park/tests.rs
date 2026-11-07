use std::path::Path;

use super::*;
use crate::git::fixture::Repo;

/// A repository in the middle of a merge of `side` into `main`: `a.txt` and `b.txt` conflict,
/// `b.txt` was resolved by hand and staged, `new.txt` comes in from `side`, and `notes.txt`
/// has a change of the user's of its own, not staged.
fn mid_merge(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(name)?;
    repo.commit(
        "base",
        &[
            ("a.txt", Some("a base\n")),
            ("b.txt", Some("b base\n")),
            ("notes.txt", Some("notes\n")),
        ],
    )?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit(
        "side",
        &[
            ("a.txt", Some("a side\n")),
            ("b.txt", Some("b side\n")),
            ("new.txt", Some("new from side\n")),
        ],
    )?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit(
        "main",
        &[("a.txt", Some("a main\n")), ("b.txt", Some("b main\n"))],
    )?;
    repo.git(&["branch", "other"])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err(), "it conflicts");
    write(repo.path(), "b.txt", "b by hand\n")?;
    repo.git(&["add", "b.txt"])?;
    write(repo.path(), "notes.txt", "notes, changed meanwhile\n")?;
    Ok(repo)
}

fn write(dir: &Path, file: &str, text: &str) -> Result<(), String> {
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())
}

fn read(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file)).ok()
}

/// Everything about the operation in progress that must come back as it was.
fn state(repo: &Repo) -> Result<Vec<String>, String> {
    let gd = git_dir(repo.path())?;
    Ok(vec![
        repo.git(&["ls-files", "-s"])?,
        repo.git(&["status", "--porcelain=v1", "--untracked-files=no"])?,
        repo.git(&["symbolic-ref", "HEAD"])?,
        repo.git(&["rev-parse", "HEAD"])?,
        std::fs::read_to_string(gd.join("MERGE_HEAD")).unwrap_or_default(),
        std::fs::read_to_string(gd.join("MERGE_MSG")).unwrap_or_default(),
        read(repo.path(), "a.txt").unwrap_or_default(),
        read(repo.path(), "b.txt").unwrap_or_default(),
        read(repo.path(), "new.txt").unwrap_or_default(),
        read(repo.path(), "notes.txt").unwrap_or_default(),
    ])
}

#[test]
fn a_merge_put_aside_comes_back_exactly_as_it_was() -> Result<(), String> {
    let repo = mid_merge("park-roundtrip")?;
    let before = state(&repo)?;
    assert!(before[0].contains(" 1\ta.txt") && before[0].contains(" 3\ta.txt"));
    std::fs::write(repo.path().join("untracked.txt"), "mine\n").map_err(|e| e.to_string())?;

    let parked = park(repo.path(), "p1", &["a.txt".to_owned()])?;
    assert_eq!(parked.operation, "merge");
    assert_eq!(parked.branch.as_deref(), Some("main"));
    assert_eq!(parked.theirs_name.as_deref(), Some("side"));
    assert!(parked.state_files.contains(&"MERGE_HEAD".to_owned()));
    // The worktree is free: its branch, clean, no merge in progress.
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", "MERGE_HEAD"])
            .is_err()
    );
    assert_eq!(read(repo.path(), "new.txt"), None);
    assert_eq!(read(repo.path(), "notes.txt").as_deref(), Some("notes\n"));
    assert_eq!(
        read(repo.path(), "untracked.txt").as_deref(),
        Some("mine\n"),
        "left alone"
    );
    // It is listed, and its files are a commit on the two sides, kept by a ref.
    assert_eq!(list(repo.path())?, vec![parked.clone()]);
    let parents = repo.git(&["rev-list", "--parents", "-n1", &parked.ref_name()])?;
    assert_eq!(parents.split_whitespace().count(), 3);

    // Meanwhile the user works elsewhere, and comes back.
    repo.git(&["checkout", "-q", "other"])?;
    repo.commit("elsewhere", &[("other.txt", Some("o\n"))])?;
    assert_eq!(
        blocker(repo.path(), &parked),
        None,
        "a clean worktree on any branch"
    );
    bring_back(repo.path(), &parked)?;

    assert_eq!(state(&repo)?, before);
    assert!(
        list(repo.path())?.is_empty(),
        "nothing left once it is back"
    );
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", &parked.ref_name()])
            .is_err()
    );
    // And the merge goes on as it would have.
    write(repo.path(), "a.txt", "a both\n")?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "--no-edit"])?;
    let merge = repo.git(&["rev-list", "--parents", "-n1", "HEAD"])?;
    assert_eq!(merge.split_whitespace().count(), 3, "a merge commit");
    Ok(())
}

#[test]
fn it_isnt_brought_back_over_work_or_a_moved_branch() -> Result<(), String> {
    let repo = mid_merge("park-blocked")?;
    let parked = park(repo.path(), "p2", &["a.txt".to_owned()])?;
    write(repo.path(), "notes.txt", "busy\n")?;
    assert!(blocker(repo.path(), &parked).is_some_and(|w| w.contains("Commit or stash")));
    assert!(bring_back(repo.path(), &parked).is_err());
    repo.git(&["checkout", "-q", "--", "notes.txt"])?;
    repo.commit("moved on", &[("later.txt", Some("l\n"))])?;
    assert!(blocker(repo.path(), &parked).is_some_and(|w| w.contains("moved since")));
    // Still kept, whole, to bring back once the branch is where it was.
    assert_eq!(list(repo.path())?.len(), 1);
    repo.git(&["reset", "-q", "--hard", &parked.head])?;
    bring_back(repo.path(), &parked)?;
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", "MERGE_HEAD"])
            .is_ok()
    );
    Ok(())
}

#[test]
fn a_cherry_pick_is_put_aside_too_but_not_a_rebase() -> Result<(), String> {
    let repo = Repo::new("park-pick")?;
    repo.commit("base", &[("a.txt", Some("base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    let pick = repo.commit("side", &[("a.txt", Some("side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("main\n"))])?;
    assert!(repo.git(&["cherry-pick", pick.trim()]).is_err());
    let before = repo.git(&["ls-files", "-s"])?;
    let parked = park(repo.path(), "p3", &["a.txt".to_owned()])?;
    assert_eq!(parked.operation, "cherry-pick");
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"])
            .is_err()
    );
    bring_back(repo.path(), &parked)?;
    assert_eq!(repo.git(&["ls-files", "-s"])?, before);
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"])
            .is_ok()
    );
    repo.git(&["cherry-pick", "--abort"])?;

    repo.git(&["checkout", "-q", "side"])?;
    assert!(repo.git(&["rebase", "-q", "main"]).is_err());
    let refused = park(repo.path(), "p4", &["a.txt".to_owned()]);
    assert!(refused.is_err_and(|e| e.contains("rebase")));
    assert!(
        list(repo.path())?.is_empty(),
        "nothing kept of what was refused"
    );
    Ok(())
}

#[test]
fn without_an_operation_in_progress_nothing_is_put_aside() -> Result<(), String> {
    let repo = Repo::new("park-nothing")?;
    repo.commit("base", &[("a.txt", Some("base\n"))])?;
    write(repo.path(), "a.txt", "changed\n")?;
    assert!(park(repo.path(), "p5", &[]).is_err());
    assert_eq!(
        read(repo.path(), "a.txt").as_deref(),
        Some("changed\n"),
        "untouched"
    );
    Ok(())
}
