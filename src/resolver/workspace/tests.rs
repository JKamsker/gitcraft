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
    assert!(has_markers(
        b"x\n<<<<<<< HEAD\ny\n=======\nz\n>>>>>>> side\n"
    ));
    assert!(has_markers(b"<<<<<<<\r\n"));
    assert!(!has_markers(b"a <<<<<<< b\n=======\n"));
    assert!(!has_markers(b"<<<<<<<<< not one\n"));
    assert_eq!(
        count_conflicts(b"<<<<<<< a\n>>>>>>> b\n<<<<<<< a\n>>>>>>> b\n"),
        2
    );
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
    let results = ws.check(repo.path(), &[]);
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
        ws.check(repo.path(), &[])[1].1,
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
    assert_eq!(ws.check(repo.path(), &[])[1].1, FileResult::Deleted);
    let applied = ws.apply(repo.path(), &["b.txt".to_owned()], &[]);
    assert_eq!(applied.written, vec!["b.txt".to_owned()]);
    assert!(!repo.path().join("b.txt").exists());
    ws.remove()
}

/// A file with no markers (one side deleted it, or it is binary) is the agent's only once it
/// changed it or said it is done with it: an agent that stopped early decides nothing.
#[test]
fn a_file_without_markers_is_only_applied_once_the_agent_decided() -> Result<(), String> {
    let repo = Repo::new("resolve-nomarkers")?;
    repo.commit("base", &[("gone.txt", Some("base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side edits", &[("gone.txt", Some("side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main deletes", &[("gone.txt", None)])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    let files = vec!["gone.txt".to_owned()];
    let ws = Workspace::prepare(repo.path(), &files)?;
    assert_eq!(ws.files[0].conflicts, 0);
    assert_eq!(ws.check(repo.path(), &[])[0].1, FileResult::Untouched);
    assert!(ws.apply(repo.path(), &files, &[]).written.is_empty());
    // Reported done as it is (keep side's edit): now it applies.
    assert_eq!(ws.check(repo.path(), &files)[0].1, FileResult::Resolved);
    ws.remove()
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
    assert_eq!(ws.check(repo.path(), &[])[0].1, FileResult::NotPlain);
    assert!(ws.apply(repo.path(), &files(), &[]).written.is_empty());
    let _ = std::fs::remove_file(&secret);
    ws.remove()
}
