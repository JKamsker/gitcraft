//! Restores: into a recreated worktree of the branch, never over a file there, never through
//! a link, only into a worktree's top folder; done again, they change nothing.

use super::archive_tests::archived;
use super::{Fixture, UNICODE, write};
use crate::git::worktree_cleanup::{Outcome, inspect, remove};

#[test]
fn files_come_back_into_a_recreated_worktree_once() -> Result<(), String> {
    let f = Fixture::new("restore recreated")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, UNICODE, "grüße\n".as_bytes())?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    let Outcome::Removed {
        archive: Some(listed),
        ..
    } = remove(f.repo.path(), &target, &preview, Some(&f.store))
    else {
        return Err("not removed".to_owned());
    };
    assert!(!wt.exists());
    // The branch is checked out again in a worktree at the same place.
    let again = f.add_worktree("done")?;
    assert_eq!(again, wt);
    let report = f.store.restore(&listed, &again)?;
    assert_eq!(report.created, [UNICODE]);
    assert_eq!(
        std::fs::read(again.join(UNICODE)).ok(),
        Some("grüße\n".as_bytes().to_vec())
    );
    let recorded = report.manifest.ok_or("not recorded")?;
    assert_eq!(recorded.restored.len(), 1);
    // Listed with the restore recorded; restoring again finds every file there already.
    let listed = f.store.list(None).into_iter().next().ok_or("not listed")?;
    assert_eq!(listed.manifest.restored.len(), 1);
    let report = f.store.restore(&listed, &again)?;
    assert_eq!((report.created.len(), report.same.len()), (0, 1));
    Ok(())
}

#[test]
fn a_file_already_there_is_never_overwritten() -> Result<(), String> {
    let f = Fixture::new("restore collision")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "a.txt", b"archived\n")?;
    write(&wt, "b.txt", b"same\n")?;
    let listed = archived(&f, &wt, "done")?;
    write(&wt, "a.txt", b"newer work\n")?;
    write(&wt, "a.txt.restored", b"in the way too\n")?;
    let report = f.store.restore(&listed, &wt)?;
    assert_eq!(report.same, ["b.txt"]);
    assert_eq!(
        report.renamed,
        [("a.txt".to_owned(), "a.txt.restored-2".to_owned())]
    );
    assert_eq!(
        std::fs::read(wt.join("a.txt")).ok(),
        Some(b"newer work\n".to_vec())
    );
    assert_eq!(
        std::fs::read(wt.join("a.txt.restored")).ok(),
        Some(b"in the way too\n".to_vec())
    );
    assert_eq!(
        std::fs::read(wt.join("a.txt.restored-2")).ok(),
        Some(b"archived\n".to_vec())
    );
    Ok(())
}

#[test]
fn only_a_worktrees_top_folder_takes_a_restore() -> Result<(), String> {
    let f = Fixture::new("restore top")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "a.txt", b"archived\n")?;
    let listed = archived(&f, &wt, "done")?;
    std::fs::create_dir_all(wt.join("inner")).map_err(|e| e.to_string())?;
    assert!(f.store.restore(&listed, &wt.join("inner")).is_err());
    assert!(!wt.join("inner/a.txt").exists());
    Ok(())
}

/// A folder of the archived path is a link out of the worktree now: nothing is written there.
#[cfg(unix)]
#[test]
fn a_restore_never_follows_a_link_out_of_the_worktree() -> Result<(), String> {
    let f = Fixture::new("restore link")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, UNICODE, b"archived\n")?;
    write(&wt, "top.txt", b"top\n")?;
    let listed = archived(&f, &wt, "done")?;
    std::fs::remove_dir_all(wt.join("notes ü")).map_err(|e| e.to_string())?;
    std::fs::remove_file(wt.join("top.txt")).map_err(|e| e.to_string())?;
    let outside = f.scratch.join("outside");
    std::fs::create_dir_all(&outside).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&outside, wt.join("notes ü")).map_err(|e| e.to_string())?;
    let report = f.store.restore(&listed, &wt)?;
    assert_eq!(report.created, ["top.txt"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, UNICODE);
    let escaped = std::fs::read_dir(&outside)
        .map_err(|e| e.to_string())?
        .count();
    assert_eq!(escaped, 0, "wrote through the link");
    Ok(())
}
