//! What the preview finds in a worktree, and what keeps it.

use super::{Fixture, UNICODE, write};
use crate::git::worktree_cleanup::files::{MAX_TEXT, checked_rel};
use crate::git::worktree_cleanup::{Blocker, Kind, Proof, inspect};

#[test]
fn a_clean_merged_worktree_has_nothing_keeping_it() -> Result<(), String> {
    let f = Fixture::new("clean")?;
    let wt = f.merged_worktree("done")?;
    let found = inspect(f.repo.path(), &f.target(&wt, "done")?)?;
    assert_eq!(found.blockers, []);
    assert_eq!(found.blockers_with(false), []);
    assert!(found.footprint.files > 0 && found.footprint.bytes > 0);
    assert!(found.footprint.complete);
    assert!(!found.fingerprint.is_empty());
    Ok(())
}

#[test]
fn an_unmerged_branch_is_not_merged_whatever_it_was_said() -> Result<(), String> {
    let f = Fixture::new("unmerged")?;
    let wt = f.open_worktree("open")?;
    let found = inspect(f.repo.path(), &f.target(&wt, "open")?)?;
    assert_eq!(found.blockers, [Blocker::NotMerged]);
    // Said to be patch-equivalent, which it is not either.
    let mut target = f.target(&wt, "open")?;
    target.evidence.proof = Proof::PatchEquivalent;
    assert_eq!(
        inspect(f.repo.path(), &target)?.blockers,
        [Blocker::NotMerged]
    );
    Ok(())
}

#[test]
fn changed_tracked_files_and_a_moved_branch_keep_it() -> Result<(), String> {
    let f = Fixture::new("dirty")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    write(&wt, "done.txt", b"changed\n")?;
    let found = inspect(f.repo.path(), &target)?;
    assert_eq!(found.changed, ["done.txt"]);
    assert_eq!(found.blockers, [Blocker::Changed(1)]);
    // Committed there: the branch moved past what was found merged.
    f.git(&wt, &["commit", "-qam", "more"])?;
    assert_eq!(inspect(f.repo.path(), &target)?.blockers, [Blocker::Moved]);
    Ok(())
}

#[test]
fn the_main_worktree_this_tab_and_a_stale_branch_name_are_kept() -> Result<(), String> {
    let f = Fixture::new("primary")?;
    let wt = f.merged_worktree("done")?;
    // Asked about the main worktree (which has `main`), from itself.
    let mut main = f.target(f.repo.path(), "main")?;
    main.evidence.tip = f.repo.tip("main")?;
    let found = inspect(f.repo.path(), &main)?;
    assert!(found.blockers.contains(&Blocker::Primary));
    assert!(
        found
            .blockers
            .iter()
            .any(|b| matches!(b, Blocker::InUse(_)))
    );
    // Seen from the worktree's own tab, it is in use there.
    let found = inspect(&wt, &f.target(&wt, "done")?)?;
    assert!(
        found
            .blockers
            .iter()
            .any(|b| matches!(b, Blocker::InUse(_)))
    );
    // The worktree has another branch now: the name offered is stale.
    let target = f.target(&wt, "done")?;
    f.git(&wt, &["switch", "-q", "-c", "other"])?;
    let found = inspect(f.repo.path(), &target)?;
    assert!(found.blockers.contains(&Blocker::BranchChanged));
    Ok(())
}

#[test]
fn an_operation_a_lock_or_an_agent_keeps_it() -> Result<(), String> {
    let f = Fixture::new("busy")?;
    let wt = f.merged_worktree("done")?;
    let mut target = f.target(&wt, "done")?;
    let dir = f.git(&wt, &["rev-parse", "--absolute-git-dir"])?;
    let dir = std::path::PathBuf::from(dir.trim());
    std::fs::write(dir.join("index.lock"), b"").map_err(|e| e.to_string())?;
    let found = inspect(f.repo.path(), &target)?;
    assert!(matches!(found.blockers.as_slice(), [Blocker::InUse(_)]));
    std::fs::remove_file(dir.join("index.lock")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("MERGE_HEAD"), target.evidence.tip.as_bytes())
        .map_err(|e| e.to_string())?;
    assert!(
        inspect(f.repo.path(), &target)?
            .blockers
            .contains(&Blocker::Operation)
    );
    std::fs::remove_file(dir.join("MERGE_HEAD")).map_err(|e| e.to_string())?;
    target.in_use = Some("agent task".to_owned());
    let found = inspect(f.repo.path(), &target)?;
    assert_eq!(found.blockers, [Blocker::InUse("agent task".to_owned())]);
    f.git(f.repo.path(), &["worktree", "lock", &wt.to_string_lossy()])?;
    target.in_use = None;
    let found = inspect(f.repo.path(), &target)?;
    assert!(matches!(found.blockers.as_slice(), [Blocker::Locked(_)]));
    Ok(())
}

#[test]
fn untracked_files_are_told_apart_and_ignored_ones_measured() -> Result<(), String> {
    let f = Fixture::new("untracked")?;
    f.repo.commit(".gitignore", "build/\n", "ignore")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    write(&wt, UNICODE, "grüße\n".as_bytes())?;
    write(&wt, "image.bin", b"PNG\0\x01\x02")?;
    write(&wt, "big.log", &vec![b'a'; (MAX_TEXT + 1) as usize])?;
    write(&wt, "build/out.o", &[7u8; 4096])?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", wt.join("link")).map_err(|e| e.to_string())?;
    let found = inspect(f.repo.path(), &target)?;
    let kind = |path: &str| {
        found
            .untracked
            .iter()
            .find(|u| u.path == path)
            .map(|u| u.kind.clone())
    };
    assert!(matches!(kind(UNICODE), Some(Kind::Text(_))));
    assert_eq!(kind("image.bin"), Some(Kind::Binary));
    assert_eq!(kind("big.log"), Some(Kind::TooLarge));
    #[cfg(unix)]
    assert_eq!(kind("link"), Some(Kind::Link));
    assert_eq!(found.ignored, ["build/"]);
    assert_eq!(
        (found.ignored_size.files, found.ignored_size.bytes),
        (1, 4096)
    );
    assert_eq!(found.blockers, []);
    let excluded = found.untracked.iter().filter(|u| !u.archivable()).count();
    assert_eq!(
        found.blockers_with(false),
        [Blocker::Untracked(found.untracked.len())]
    );
    assert_eq!(
        found.blockers_with(true),
        [Blocker::NotArchivable(excluded)]
    );
    Ok(())
}

#[test]
fn only_plain_relative_names_are_accepted() {
    assert_eq!(checked_rel("a/b c/ü.txt"), Some(vec!["a", "b c", "ü.txt"]));
    for bad in [
        "",
        "/etc/passwd",
        "../x",
        "a/../b",
        "a//b",
        "./a",
        "a\\b",
        "C:/x",
        ".git/config",
        "sub/.GIT/hooks",
        "a\0b",
    ] {
        assert_eq!(checked_rel(bad), None, "{bad:?}");
    }
}
