use super::*;
use crate::git::fixture::Repo;

/// A repository in the middle of a merge of `side` into `main` that conflicts in `a.txt` and
/// `b.txt`.
fn conflicted(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(name)?;
    repo.commit(
        "base",
        &[
            ("a.txt", Some("one\nbase\nthree\n")),
            ("b.txt", Some("b\n")),
        ],
    )?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit(
        "side",
        &[
            ("a.txt", Some("one\nside\nthree\n")),
            ("b.txt", Some("b side\n")),
        ],
    )?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit(
        "main",
        &[
            ("a.txt", Some("one\nmain\nthree\n")),
            ("b.txt", Some("b main\n")),
        ],
    )?;
    // It conflicts: git says so by failing.
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    Ok(repo)
}

fn files() -> Vec<String> {
    vec!["a.txt".to_owned(), "b.txt".to_owned()]
}

#[test]
fn markers_are_found_at_the_start_of_a_line_only() {
    let has = |text: &[u8]| has_markers(text, MARKER_SIZE);
    assert!(has(b"x\n<<<<<<< HEAD\ny\n=======\nz\n>>>>>>> side\n"));
    assert!(has(b"<<<<<<<\r\n"));
    assert!(!has(b"a <<<<<<< b\n=======\n"));
    assert!(!has(b"<<<<<<<<< not one\n"));
    // The base's marker, left behind, is one too; a lone separator line isn't.
    assert!(has(b"a\n||||||| base\nb\n"));
    assert!(!has(b"Title\n=======\n"));
    assert_eq!(
        count_conflicts(b"<<<<<<< a\n>>>>>>> b\n<<<<<<< a\n>>>>>>> b\n", MARKER_SIZE),
        2
    );
    // Markers of the size an attribute sets, and only those.
    assert!(has_markers(b"<<<<<<<<<< ours\n", 10));
    assert!(!has_markers(b"<<<<<<< ours\n", 10));
    assert_eq!(count_conflicts(b"<<<<<<<<<< a\n>>>>>>>>>> b\n", 10), 1);
}

/// A file the user already resolved is in the agent's worktree as they resolved it, and not
/// in conflict there: only what the agent is given is.
#[test]
fn files_the_user_resolved_are_there_as_they_resolved_them() -> Result<(), String> {
    let repo = conflicted("resolve-context")?;
    std::fs::write(repo.path().join("b.txt"), "b mine\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "b.txt"])?;
    let ws = Workspace::prepare(repo.path(), &["a.txt".to_owned()])?;
    let b = std::fs::read_to_string(ws.dir().join("b.txt")).map_err(|e| e.to_string())?;
    assert_eq!(b, "b mine\n");
    assert_eq!(unmerged(ws.dir())?, ["a.txt"]);
    ws.remove()
}

/// A cherry-pick of a merge commit is replayed by a parent (`-m`): without one git refuses it,
/// and the agent would get no conflict to look at.
#[test]
fn a_cherry_pick_of_a_merge_is_replayed() -> Result<(), String> {
    let repo = conflicted("resolve-pick-merge")?;
    repo.git(&["merge", "--abort"])?;
    repo.git(&["checkout", "-q", "-b", "merged", "side~1"])?;
    repo.commit("other", &[("c.txt", Some("c\n"))])?;
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"])?;
    repo.git(&["checkout", "-q", "main"])?;
    assert!(repo.git(&["cherry-pick", "-m", "1", "merged"]).is_err());
    let ws = Workspace::prepare(repo.path(), &files())?;
    assert_eq!(ws.sides.operation, "cherry-pick");
    assert_eq!(unmerged(ws.dir())?, files());
    ws.remove()
}

/// A replay git refuses stops the run before the agent starts.
#[test]
fn a_replay_git_refuses_is_an_error() -> Result<(), String> {
    let repo = Repo::new("resolve-replay-refused")?;
    repo.commit("base", &[("a.txt", Some("a\n"))])?;
    let args = ["cherry-pick", "--no-commit", "no-such-commit"].map(str::to_owned);
    assert!(replay(repo.path(), &args).is_err());
    Ok(())
}

/// What is written replaces the file whole: no file of gitcraft's is left beside it, and an
/// executable stays one.
#[test]
fn a_file_is_written_whole_and_keeps_its_mode() -> Result<(), String> {
    let repo = Repo::new("resolve-write")?;
    let dir = repo.path();
    std::fs::create_dir_all(dir.join("bin")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("bin/run"), "old\n").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exec = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(dir.join("bin/run"), exec).map_err(|e| e.to_string())?;
    }
    write(dir, "bin/run", Some(b"new\n"))?;
    assert_eq!(read(dir, "bin/run")?.as_deref(), Some(&b"new\n"[..]));
    let left = std::fs::read_dir(dir.join("bin")).map_err(|e| e.to_string())?;
    assert_eq!(left.count(), 1, "nothing left beside it");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(dir.join("bin/run")).map_err(|e| e.to_string())?;
        assert_eq!(meta.permissions().mode() & 0o777, 0o755);
    }
    // A file that can't be written in its place leaves what is there as it was.
    std::fs::create_dir_all(dir.join("bin/dir/x")).map_err(|e| e.to_string())?;
    assert!(write(dir, "bin/dir", Some(b"x")).is_err());
    assert!(dir.join("bin/dir/x").is_dir());
    Ok(())
}

/// On Unix a `\` is part of a file's name, not a folder's end: the file is found as such.
#[cfg(unix)]
#[test]
fn a_backslash_in_a_name_is_part_of_it_on_unix() -> Result<(), String> {
    let repo = Repo::new("resolve-backslash")?;
    std::fs::write(repo.path().join(r"a\b.txt"), "x\n").map_err(|e| e.to_string())?;
    assert_eq!(read(repo.path(), r"a\b.txt")?.as_deref(), Some(&b"x\n"[..]));
    Ok(())
}

/// A file whose markers an attribute makes longer (`conflict-marker-size=10`) is counted as
/// conflicted, and isn't resolved while they are in it.
#[test]
fn markers_of_the_size_set_for_a_file_are_recognized() -> Result<(), String> {
    let repo = Repo::new("resolve-marker-size")?;
    repo.commit(
        "base",
        &[
            (".gitattributes", Some("a.txt conflict-marker-size=10\n")),
            ("a.txt", Some("one\nbase\nthree\n")),
        ],
    )?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side", &[("a.txt", Some("one\nside\nthree\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("one\nmain\nthree\n"))])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    let files = vec!["a.txt".to_owned()];
    let ws = Workspace::prepare(repo.path(), &files)?;
    assert_eq!((ws.files[0].conflicts, ws.files[0].marker), (1, 10));
    assert_eq!(
        ws.check(Some(repo.path()), &files)[0].1,
        FileResult::StillConflicted
    );
    ws.remove()
}

/// A file that had no markers isn't resolved by the agent putting some in it.
#[test]
fn markers_the_agent_put_in_a_markerless_file_keep_it_conflicted() -> Result<(), String> {
    let repo = deleted_by_main("resolve-new-markers")?;
    let files = vec!["gone.txt".to_owned()];
    let ws = Workspace::prepare(repo.path(), &files)?;
    std::fs::write(
        ws.dir().join("gone.txt"),
        "<<<<<<< HEAD\n=======\nside\n>>>>>>> side\n",
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(
        ws.check(Some(repo.path()), &files)[0].1,
        FileResult::StillConflicted
    );
    assert!(ws.apply(repo.path(), &files, &files).written.is_empty());
    ws.remove()
}

/// A merge where `main` deleted `gone.txt` and `side` edited it: no markers.
fn deleted_by_main(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(name)?;
    repo.commit("base", &[("gone.txt", Some("base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side edits", &[("gone.txt", Some("side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main deletes", &[("gone.txt", None)])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    Ok(repo)
}

#[test]
fn the_agent_gets_the_merge_replayed_and_the_files_as_they_are_in_the_checkout()
-> Result<(), String> {
    let repo = conflicted("resolve-prepare")?;
    // The user already resolved part of b.txt by hand: the agent starts from that.
    std::fs::write(
        repo.path().join("b.txt"),
        "<<<<<<< HEAD\nmine\n=======\nb side\n>>>>>>> side\n",
    )
    .map_err(|e| e.to_string())?;
    let ws = Workspace::prepare(repo.path(), &files())?;
    assert_eq!(ws.sides.operation, "merge");
    assert_eq!(ws.sides.ours_name, "main");
    assert_eq!(ws.sides.theirs_name.as_deref(), Some("side"));
    assert_eq!(ws.files[0].conflicts, 1);
    let b = std::fs::read_to_string(ws.dir().join("b.txt")).map_err(|e| e.to_string())?;
    assert!(b.contains("mine"), "{b}");
    // The merge is in progress there too, so the agent can look at both sides.
    assert!(
        git::run(
            Some(ws.dir()),
            &["rev-parse", "-q", "--verify", "MERGE_HEAD"]
        )
        .is_ok()
    );
    // And it is nowhere among the worktrees the user sees.
    let listed = git::worktrees(repo.path(), None)?;
    assert_eq!(listed.len(), 1, "{listed:?}");
    let dir = ws.dir().to_path_buf();
    ws.remove()?;
    assert!(!dir.exists());
    Ok(())
}

#[test]
fn only_files_resolved_and_untouched_by_the_user_are_applied() -> Result<(), String> {
    let repo = conflicted("resolve-apply")?;
    let ws = Workspace::prepare(repo.path(), &files())?;
    // The agent resolves a.txt, leaves b.txt half done.
    std::fs::write(ws.dir().join("a.txt"), "one\nmain and side\nthree\n")
        .map_err(|e| e.to_string())?;
    let results = ws.check(Some(repo.path()), &[]);
    assert_eq!(
        results,
        vec![
            ("a.txt".to_owned(), FileResult::Resolved),
            ("b.txt".to_owned(), FileResult::StillConflicted),
        ]
    );
    let applied = ws.apply(repo.path(), &files(), &[]);
    assert_eq!(applied.written, vec!["a.txt".to_owned()]);
    assert!(applied.failed.is_empty());
    let a = std::fs::read_to_string(repo.path().join("a.txt")).map_err(|e| e.to_string())?;
    assert_eq!(a, "one\nmain and side\nthree\n");
    // Now the user edits b.txt themselves while the agent finishes it: theirs stays.
    std::fs::write(ws.dir().join("b.txt"), "b both\n").map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("b.txt"), "b by hand\n").map_err(|e| e.to_string())?;
    assert_eq!(
        ws.check(Some(repo.path()), &[])[1].1,
        FileResult::ChangedMeanwhile
    );
    // a.txt was applied before: it too now differs from what the agent started from.
    assert!(ws.apply(repo.path(), &files(), &[]).written.is_empty());
    let b = std::fs::read_to_string(repo.path().join("b.txt")).map_err(|e| e.to_string())?;
    assert_eq!(b, "b by hand\n");
    ws.remove()
}

#[test]
fn a_file_the_agent_deleted_is_deleted_and_paths_stay_inside() -> Result<(), String> {
    let repo = conflicted("resolve-delete")?;
    assert!(Workspace::prepare(repo.path(), &["../x".to_owned()]).is_err());
    assert!(Workspace::prepare(repo.path(), &[]).is_err());
    let ws = Workspace::prepare(repo.path(), &files())?;
    std::fs::remove_file(ws.dir().join("b.txt")).map_err(|e| e.to_string())?;
    assert_eq!(ws.check(Some(repo.path()), &[])[1].1, FileResult::Deleted);
    let applied = ws.apply(repo.path(), &["b.txt".to_owned()], &[]);
    assert_eq!(applied.written, vec!["b.txt".to_owned()]);
    assert!(!repo.path().join("b.txt").exists());
    ws.remove()
}

/// A file with no markers (one side deleted it, or it is binary) is the agent's only once it
/// changed it or said it is done with it: an agent that stopped early decides nothing.
#[test]
fn a_file_without_markers_is_only_applied_once_the_agent_decided() -> Result<(), String> {
    let repo = deleted_by_main("resolve-nomarkers")?;
    let files = vec!["gone.txt".to_owned()];
    let ws = Workspace::prepare(repo.path(), &files)?;
    assert_eq!(ws.files[0].conflicts, 0);
    assert_eq!(ws.check(Some(repo.path()), &[])[0].1, FileResult::Untouched);
    assert!(ws.apply(repo.path(), &files, &[]).written.is_empty());
    // Reported done as it is (keep side's edit): now it applies.
    assert_eq!(
        ws.check(Some(repo.path()), &files)[0].1,
        FileResult::Resolved
    );
    assert_eq!(ws.apply(repo.path(), &files, &files).written, files);
    let gone = std::fs::read_to_string(repo.path().join("gone.txt")).map_err(|e| e.to_string())?;
    assert_eq!(gone, "side\n");
    ws.remove()
}

/// The work kept (with a merge put aside) reads back as it was left: each file as given and
/// as the agent left it, deletions and what isn't a plain file included; one whose saved copy
/// is missing makes the whole of it unreadable, never a deletion.
#[test]
fn work_kept_reads_back_as_it_was_left() -> Result<(), String> {
    let repo = conflicted("resolve-keep")?;
    std::fs::write(repo.path().join("c.txt"), "c\n").map_err(|e| e.to_string())?;
    let given = vec!["a.txt".to_owned(), "b.txt".to_owned(), "c.txt".to_owned()];
    let ws = Workspace::prepare(repo.path(), &given)?;
    std::fs::write(ws.dir().join("a.txt"), "one\nboth\nthree\n").map_err(|e| e.to_string())?;
    std::fs::remove_file(ws.dir().join("b.txt")).map_err(|e| e.to_string())?;
    // Not a file: a folder where c.txt was.
    std::fs::remove_file(ws.dir().join("c.txt")).map_err(|e| e.to_string())?;
    std::fs::create_dir(ws.dir().join("c.txt")).map_err(|e| e.to_string())?;
    let to = repo.path().join(".git/resolver-kept");
    ws.keep(&to)?;
    ws.remove()?;
    let kept = Workspace::load(&to)?;
    assert!(kept.is_kept());
    let (before, after) = kept.before_and_after("a.txt")?;
    assert!(before.is_some_and(|b| has_markers(b, MARKER_SIZE)));
    assert_eq!(after.as_deref(), Some(&b"one\nboth\nthree\n"[..]));
    assert_eq!(kept.before_and_after("b.txt")?.1, None);
    assert_eq!(
        kept.check(None, &[]),
        vec![
            ("a.txt".to_owned(), FileResult::Resolved),
            ("b.txt".to_owned(), FileResult::Deleted),
            ("c.txt".to_owned(), FileResult::NotPlain),
        ]
    );
    // The agent's a.txt gone from what is kept: not taken for a deletion.
    std::fs::remove_file(to.join("resolved/a.txt")).map_err(|e| e.to_string())?;
    assert!(Workspace::load(&to).is_err());
    kept.remove()
}

/// The agent can't have a secret elsewhere applied (and so committed) by linking to it.
#[cfg(unix)]
#[test]
fn a_link_the_agent_made_is_never_applied() -> Result<(), String> {
    let repo = conflicted("resolve-link")?;
    let ws = Workspace::prepare(repo.path(), &files())?;
    let secret = repo.path().join("../resolve-link-secret");
    std::fs::write(&secret, "token\n").map_err(|e| e.to_string())?;
    std::fs::remove_file(ws.dir().join("a.txt")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&secret, ws.dir().join("a.txt")).map_err(|e| e.to_string())?;
    assert_eq!(ws.check(Some(repo.path()), &[])[0].1, FileResult::NotPlain);
    assert!(ws.apply(repo.path(), &files(), &[]).written.is_empty());
    let _ = std::fs::remove_file(&secret);
    ws.remove()
}
