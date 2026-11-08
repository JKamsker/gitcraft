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
    // It is listed, and its files are a commit on the two sides and what was staged (the
    // user's change to `notes.txt` isn't), kept by a ref.
    assert_eq!(list(repo.path())?, vec![parked.clone()]);
    let parents = repo.git(&["rev-list", "--parents", "-n1", &parked.ref_name()])?;
    assert_eq!(parents.split_whitespace().count(), 4);

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

/// Something of the user's where the merge's new file goes: bringing it back stops before
/// writing anything, and goes through once it is moved.
fn in_the_way(name: &str, place: impl Fn(&Repo) -> Result<(), String>) -> Result<(), String> {
    let repo = mid_merge(name)?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "w1", &["a.txt".to_owned()])?;
    place(&repo)?;
    let refused = bring_back(repo.path(), &parked);
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("move them")),
        "{refused:?}"
    );
    assert_eq!(list(repo.path())?.len(), 1, "still put aside, whole");
    assert!(!merging_now(&repo), "nothing of the merge came back");
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    let _ = std::fs::remove_file(repo.path().join("new.txt"));
    let _ = std::fs::remove_dir_all(repo.path().join("new.txt"));
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

fn merging_now(repo: &Repo) -> bool {
    repo.git(&["rev-parse", "-q", "--verify", "MERGE_HEAD"])
        .is_ok()
}

#[test]
fn an_untracked_file_in_the_way_is_never_written_over() -> Result<(), String> {
    in_the_way("park-way-file", |repo| {
        write(repo.path(), "new.txt", "USER DATA\n")?;
        Ok(())
    })
}

#[test]
fn an_ignored_file_in_the_way_is_never_written_over() -> Result<(), String> {
    in_the_way("park-way-ignored", |repo| {
        let exclude = repo.path().join(".git/info/exclude");
        std::fs::write(&exclude, "new.txt\n").map_err(|e| e.to_string())?;
        write(repo.path(), "new.txt", "IGNORED BUT MINE\n")?;
        Ok(())
    })
}

#[test]
fn a_folder_in_the_way_is_never_removed() -> Result<(), String> {
    in_the_way("park-way-dir", |repo| {
        std::fs::create_dir_all(repo.path().join("new.txt")).map_err(|e| e.to_string())?;
        write(repo.path(), "new.txt/notes.txt", "notes\n")?;
        Ok(())
    })
}

/// The user's file stays: checked before it is moved away by `in_the_way`.
#[test]
fn what_was_in_the_way_is_left_as_it_was() -> Result<(), String> {
    let repo = mid_merge("park-way-kept")?;
    let parked = park(repo.path(), "w2", &["a.txt".to_owned()])?;
    write(repo.path(), "new.txt", "USER DATA\n")?;
    assert!(bring_back(repo.path(), &parked).is_err());
    assert_eq!(read(repo.path(), "new.txt").as_deref(), Some("USER DATA\n"));
    Ok(())
}

#[test]
fn files_git_is_told_not_to_look_at_keep_the_merge_from_being_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-assume")?;
    repo.git(&["update-index", "--assume-unchanged", "notes.txt"])?;
    let before = state(&repo)?;
    let refused = park(repo.path(), "a1", &["a.txt".to_owned()]);
    assert!(refused.is_err_and(|e| e.contains("assume-unchanged")));
    assert_eq!(state(&repo)?, before, "nothing changed");
    assert!(list(repo.path())?.is_empty(), "nothing kept");
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", "refs/gitcraft/parked/a1"])
            .is_err()
    );
    Ok(())
}

#[test]
fn a_locked_index_stops_it_and_it_stays_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-locked")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "l1", &["a.txt".to_owned()])?;
    let lock = repo.path().join(".git/index.lock");
    std::fs::write(&lock, "").map_err(|e| e.to_string())?;
    let refused = bring_back(repo.path(), &parked);
    // What stopped it is said, not files in the way that aren't.
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("index.lock") && !e.contains("doesn't track")),
        "{refused:?}"
    );
    std::fs::remove_file(&lock).map_err(|e| e.to_string())?;
    assert_eq!(list(repo.path())?.len(), 1);
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

#[test]
fn a_merge_dropped_keeps_its_files_on_a_branch() -> Result<(), String> {
    let repo = mid_merge("park-drop")?;
    let parked = park(repo.path(), "d1", &["a.txt".to_owned()])?;
    let branch = drop_to_branch(repo.path(), &parked)?;
    assert!(branch.starts_with("parked/merge-"), "{branch}");
    assert!(list(repo.path())?.is_empty());
    let a = repo.git(&["show", &format!("{branch}:a.txt")])?;
    assert!(a.contains("<<<<<<<"), "the conflicted file as it was: {a}");
    let notes = repo.git(&["show", &format!("{branch}:notes.txt")])?;
    assert_eq!(notes, "notes, changed meanwhile\n", "the user's change too");
    Ok(())
}

/// `side` merged into `main` in `repo`, `a.txt` conflicting; the base having `base` too, and
/// `side` changing `side` too.
fn conflict(
    repo: &Repo,
    base: &[(&str, Option<&str>)],
    side: &[(&str, Option<&str>)],
) -> Result<(), String> {
    repo.commit("base", &[&[("a.txt", Some("a base\n"))], base].concat())?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side", &[&[("a.txt", Some("a side\n"))], side].concat())?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("a main\n"))])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err(), "it conflicts");
    Ok(())
}

#[test]
fn a_file_named_like_a_submodule_mode_is_no_submodule() -> Result<(), String> {
    let repo = mid_merge("park-160000")?;
    write(repo.path(), "160000.txt", "a number\n")?;
    repo.git(&["add", "160000.txt"])?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "m1", &["a.txt".to_owned()])?;
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

#[test]
fn a_submodule_with_changes_of_its_own_keeps_the_merge_from_being_put_aside() -> Result<(), String>
{
    let repo = Repo::new("park-sub")?;
    let sub = repo.path().join("sub");
    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    let in_sub = |args: &[&str]| git::run(Some(&sub), args);
    in_sub(&["init", "-q"])?;
    write(&sub, "s.txt", "s\n")?;
    in_sub(&["add", "s.txt"])?;
    in_sub(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-q",
        "-m",
        "s",
    ])?;
    conflict(&repo, &[], &[])?;
    assert!(repo.git(&["ls-files", "-s", "sub"])?.starts_with("160000"));
    write(&sub, "s.txt", "the submodule's own change\n")?;
    let refused = park(repo.path(), "s1", &["a.txt".to_owned()]);
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("submodule")),
        "{refused:?}"
    );
    assert!(merging_now(&repo), "nothing changed");
    assert_eq!(
        read(&sub, "s.txt").as_deref(),
        Some("the submodule's own change\n")
    );
    Ok(())
}

#[test]
fn an_ignored_file_where_its_branch_has_one_stops_the_checkout() -> Result<(), String> {
    let repo = mid_merge("park-ignored-checkout")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "i1", &["a.txt".to_owned()])?;
    repo.git(&["checkout", "-q", "-b", "elsewhere"])?;
    repo.git(&["rm", "-q", "notes.txt"])?;
    repo.git(&["commit", "-q", "-m", "no notes"])?;
    let exclude = repo.path().join(".git/info/exclude");
    std::fs::write(&exclude, "notes.txt\n").map_err(|e| e.to_string())?;
    write(repo.path(), "notes.txt", "IGNORED BUT MINE\n")?;
    assert!(bring_back(repo.path(), &parked).is_err());
    assert_eq!(
        read(repo.path(), "notes.txt").as_deref(),
        Some("IGNORED BUT MINE\n")
    );
    assert_eq!(list(repo.path())?.len(), 1, "still put aside");
    std::fs::remove_file(repo.path().join("notes.txt")).map_err(|e| e.to_string())?;
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

#[test]
fn what_was_staged_is_kept_from_gc_and_on_the_branch_it_is_dropped_to() -> Result<(), String> {
    let repo = mid_merge("park-staged")?;
    write(repo.path(), "notes.txt", "staged notes\n")?;
    repo.git(&["add", "notes.txt"])?;
    write(repo.path(), "notes.txt", "notes on disk\n")?;
    let staged = repo.rev(":notes.txt")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "g1", &["a.txt".to_owned()])?;
    let objects = repo.git(&["rev-list", "--objects", &parked.ref_name()])?;
    assert!(objects.contains(&staged), "reachable from its ref");
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);

    let parked = park(repo.path(), "g2", &["a.txt".to_owned()])?;
    let branch = drop_to_branch(repo.path(), &parked)?;
    let kept = repo.git(&["show", &format!("{branch}^3:notes.txt")])?;
    assert_eq!(kept, "staged notes\n");
    let files = repo.git(&["show", &format!("{branch}:notes.txt")])?;
    assert_eq!(files, "notes on disk\n");
    Ok(())
}

#[test]
fn a_sparse_checkout_keeps_it_from_coming_back_until_it_is_off() -> Result<(), String> {
    let repo = mid_merge("park-sparse")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "c1", &["a.txt".to_owned()])?;
    repo.git(&["sparse-checkout", "set", "--no-cone", "/a.txt", "/b.txt"])?;
    let refused = bring_back(repo.path(), &parked);
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("sparse")),
        "{refused:?}"
    );
    assert_eq!(list(repo.path())?.len(), 1, "still put aside");
    assert!(!merging_now(&repo));
    repo.git(&["sparse-checkout", "disable"])?;
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

#[test]
fn a_merge_on_a_detached_head_comes_back_detached() -> Result<(), String> {
    let repo = Repo::new("park-detached")?;
    repo.commit("base", &[("a.txt", Some("base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side", &[("a.txt", Some("side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("main\n"))])?;
    repo.git(&["checkout", "-q", "--detach"])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    let before = repo.git(&["ls-files", "-s"])?;
    let parked = park(repo.path(), "h1", &["a.txt".to_owned()])?;
    assert_eq!(parked.branch, None);
    // A branch at the very commit checked out meanwhile.
    repo.git(&["checkout", "-q", "main"])?;
    bring_back(repo.path(), &parked)?;
    assert!(
        repo.git(&["symbolic-ref", "-q", "HEAD"]).is_err(),
        "detached again"
    );
    assert!(merging_now(&repo));
    assert_eq!(repo.git(&["ls-files", "-s"])?, before);
    Ok(())
}

#[test]
fn a_tracked_file_the_merge_makes_a_folder_comes_back() -> Result<(), String> {
    let repo = Repo::new("park-file-folder")?;
    let side = [("config", None), ("config/x", Some("x\n"))];
    conflict(&repo, &[("config", Some("cfg\n"))], &side)?;
    let before = repo.git(&["ls-files", "-s"])?;
    // Something of the user's in that folder: the reset would remove it with the folder.
    write(repo.path(), "config/mine.txt", "MINE\n")?;
    let refused = park(repo.path(), "f1", &["a.txt".to_owned()]);
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("config")),
        "{refused:?}"
    );
    assert_eq!(
        read(repo.path(), "config/mine.txt").as_deref(),
        Some("MINE\n")
    );
    std::fs::remove_file(repo.path().join("config/mine.txt")).map_err(|e| e.to_string())?;

    let parked = park(repo.path(), "f2", &["a.txt".to_owned()])?;
    assert_eq!(read(repo.path(), "config").as_deref(), Some("cfg\n"));
    bring_back(repo.path(), &parked)?;
    assert_eq!(repo.git(&["ls-files", "-s"])?, before);
    assert_eq!(read(repo.path(), "config/x").as_deref(), Some("x\n"));
    Ok(())
}

#[test]
fn a_changed_file_comes_back_byte_for_byte_whatever_the_filters() -> Result<(), String> {
    let repo = mid_merge("park-bytes")?;
    let attributes = repo.path().join(".git/info/attributes");
    std::fs::write(&attributes, "notes.txt text eol=crlf\n").map_err(|e| e.to_string())?;
    let parked = park(repo.path(), "b1", &["a.txt".to_owned()])?;
    bring_back(repo.path(), &parked)?;
    let notes = std::fs::read(repo.path().join("notes.txt")).map_err(|e| e.to_string())?;
    assert_eq!(notes, b"notes, changed meanwhile\n", "not with CRLF");
    Ok(())
}

#[test]
fn a_file_the_merge_deleted_on_disk_again_keeps_it_from_being_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-recreated")?;
    // Out of the merge's index, the file on disk the user's own: the reset would write the
    // branch's over it.
    repo.git(&["rm", "-q", "--cached", "notes.txt"])?;
    let before = state(&repo)?;
    let refused = park(repo.path(), "r1", &["a.txt".to_owned()]);
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("notes.txt")),
        "{refused:?}"
    );
    assert_eq!(state(&repo)?, before, "nothing changed");
    assert!(list(repo.path())?.is_empty(), "nothing kept");
    Ok(())
}

#[test]
fn a_merge_put_aside_already_isnt_written_over() -> Result<(), String> {
    let repo = mid_merge("park-taken")?;
    let taken = crate::agent::repo_file(repo.path(), "parked/t1")?;
    std::fs::create_dir_all(&taken).map_err(|e| e.to_string())?;
    std::fs::write(taken.join("index"), "kept before").map_err(|e| e.to_string())?;
    let before = state(&repo)?;
    assert!(park(repo.path(), "t1", &["a.txt".to_owned()]).is_err());
    assert_eq!(read(&taken, "index").as_deref(), Some("kept before"));
    assert_eq!(state(&repo)?, before, "nothing changed");
    Ok(())
}

#[test]
fn a_failure_once_it_is_coming_back_leaves_it_clean_and_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-half")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "k1", &["a.txt".to_owned()])?;
    let dir = folder(repo.path(), "k1")?;
    let (raw, away) = (dir.join("raw/a.txt"), dir.join("a.txt.away"));
    std::fs::rename(&raw, &away).map_err(|e| e.to_string())?;
    // The index goes back, then a file kept can't be.
    assert!(bring_back(repo.path(), &parked).is_err());
    assert!(!merging_now(&repo));
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    assert_eq!(read(repo.path(), "new.txt"), None);
    assert_eq!(list(repo.path())?.len(), 1, "still put aside");
    std::fs::rename(&away, &raw).map_err(|e| e.to_string())?;
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

#[test]
fn restoration_rejects_both_hidden_index_flags() -> Result<(), String> {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let repo = mid_merge(&format!("park-hidden-{flag}"))?;
        let parked = park(repo.path(), "hidden", &["a.txt".into()])?;
        repo.git(&["update-index", flag, "notes.txt"])?;
        write(repo.path(), "notes.txt", "hidden user edit\n")?;
        assert!(
            repo.git(&["status", "--porcelain", "--untracked-files=no"])?
                .is_empty()
        );
        assert!(bring_back(repo.path(), &parked).is_err());
        assert_eq!(
            read(repo.path(), "notes.txt").as_deref(),
            Some("hidden user edit\n")
        );
        assert_eq!(list(repo.path())?, vec![parked]);
    }
    Ok(())
}

#[test]
fn freshness_rejects_raw_changes_that_git_normalizes_away() -> Result<(), String> {
    let repo = mid_merge("park-raw-freshness")?;
    write(repo.path(), ".gitattributes", "notes.txt text eol=lf\n")?;
    repo.git(&["add", ".gitattributes"])?;
    let gd = git_dir(repo.path())?;
    let dir = folder(repo.path(), "raw-freshness")?;
    reserve(&dir)?;
    let (_, theirs) = operation(repo.path(), &gd)?;
    let parked = keep(
        repo.path(),
        &gd,
        &dir,
        "raw-freshness",
        "merge",
        &theirs,
        &["a.txt".into()],
    )?;
    assert!(unchanged(repo.path(), &gd, &dir, &parked.snapshot));
    write(repo.path(), "notes.txt", "notes, changed meanwhile\r\n")?;
    // Same filtered tree, different user bytes: the pre-reset guard must stop.
    let tree = trees(repo.path(), &gd.join("index"))?.1;
    assert_eq!(
        tree,
        trimmed(run(
            repo.path(),
            &["rev-parse", &format!("{}^{{tree}}", parked.snapshot)]
        )?)
    );
    assert!(!unchanged(repo.path(), &gd, &dir, &parked.snapshot));
    Ok(())
}

#[test]
fn dropping_to_a_branch_keeps_accessible_byte_exact_recovery() -> Result<(), String> {
    let repo = mid_merge("park-raw-recovery")?;
    write(repo.path(), ".gitattributes", "notes.txt text eol=lf\n")?;
    repo.git(&["add", ".gitattributes"])?;
    let raw = b"notes, changed meanwhile\r\n";
    std::fs::write(repo.path().join("notes.txt"), raw).map_err(|e| e.to_string())?;
    let parked = park(repo.path(), "raw-recovery", &["a.txt".into()])?;
    let branch = drop_to_branch(repo.path(), &parked)?;
    let archived = crate::agent::repo_file(repo.path(), "parked-recovery/raw-recovery/parked")?;
    assert_eq!(
        std::fs::read(archived.join("raw/notes.txt")).map_err(|e| e.to_string())?,
        raw
    );
    assert!(archived.join("index").is_file());
    assert!(archived.join("parked.json").is_file());
    let description = repo.git(&["config", &format!("branch.{branch}.description")])?;
    assert!(description.contains(&archived.display().to_string()));
    assert!(list(repo.path())?.is_empty());
    assert!(
        repo.git(&["show", &format!("{branch}:notes.txt")])?
            .ends_with("\n")
    );
    Ok(())
}

#[test]
fn a_filtered_clean_file_is_also_kept_byte_exact() -> Result<(), String> {
    let repo = mid_merge("park-clean-raw")?;
    write(repo.path(), ".gitattributes", "notes.txt text eol=lf\n")?;
    repo.git(&["add", ".gitattributes"])?;
    write(repo.path(), "notes.txt", "notes\r\n")?;
    assert!(repo.git(&["diff", "--", "notes.txt"])?.is_empty());
    let parked = park(repo.path(), "clean-raw", &["a.txt".into()])?;
    assert!(parked.raw.contains(&"notes.txt".into()));
    bring_back(repo.path(), &parked)?;
    assert_eq!(
        std::fs::read(repo.path().join("notes.txt")).map_err(|e| e.to_string())?,
        b"notes\r\n"
    );
    Ok(())
}

#[test]
fn recovery_archive_collision_keeps_both_originals() -> Result<(), String> {
    let repo = mid_merge("park-recovery-collision")?;
    let parked = park(repo.path(), "collision", &["a.txt".into()])?;
    let archive = crate::agent::repo_file(repo.path(), "parked-recovery/collision")?;
    std::fs::create_dir_all(&archive).map_err(|e| e.to_string())?;
    std::fs::write(archive.join("existing"), b"mine").map_err(|e| e.to_string())?;
    assert!(drop_to_branch(repo.path(), &parked).is_err());
    assert_eq!(
        std::fs::read(archive.join("existing")).map_err(|e| e.to_string())?,
        b"mine"
    );
    assert_eq!(list(repo.path())?, vec![parked.clone()]);
    assert!(folder(repo.path(), &parked.id)?.join("raw/a.txt").is_file());
    Ok(())
}
