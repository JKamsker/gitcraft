//! Removing previewed worktrees: only what was previewed and nothing keeps, untracked files
//! archived first when asked, kept whole when anything fails, and retried after.

use super::{Fixture, UNICODE, write};
use crate::git::worktree_cleanup::{Blocker, Outcome, inspect, remove};

#[test]
fn a_clean_merged_worktree_goes_and_its_branch_stays() -> Result<(), String> {
    let f = Fixture::new("remove clean")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    let outcome = remove(f.repo.path(), &target, &preview, None);
    assert!(
        matches!(outcome, Outcome::Removed { archive: None, .. }),
        "{outcome:?}"
    );
    assert!(!wt.exists());
    assert_eq!(f.repo.tip("refs/heads/done")?, target.evidence.tip);
    let listed = f.git(f.repo.path(), &["worktree", "list", "--porcelain"])?;
    assert!(!listed.contains("tree done"));
    Ok(())
}

#[test]
fn changed_unmerged_or_untracked_worktrees_stay_whole() -> Result<(), String> {
    let f = Fixture::new("remove kept")?;
    let dirty = f.merged_worktree("dirty")?;
    let open = f.open_worktree("open")?;
    let loose = f.merged_worktree("loose")?;
    write(&dirty, "dirty.txt", b"edited\n")?;
    write(&loose, UNICODE, b"keep me\n")?;
    for (wt, branch) in [(&dirty, "dirty"), (&open, "open"), (&loose, "loose")] {
        let target = f.target(wt, branch)?;
        let preview = inspect(f.repo.path(), &target)?;
        let outcome = remove(f.repo.path(), &target, &preview, None);
        assert!(matches!(outcome, Outcome::Kept(_)), "{branch}: {outcome:?}");
        assert!(
            wt.join(format!("{branch}.txt")).exists(),
            "{branch} lost files"
        );
    }
    assert!(loose.join(UNICODE).exists());
    Ok(())
}

#[test]
fn a_change_after_the_preview_keeps_it_until_looked_at_again() -> Result<(), String> {
    let f = Fixture::new("remove changed")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    write(&wt, "new.txt", b"written after the preview\n")?;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    assert_eq!(outcome, Outcome::Changed);
    assert!(wt.join("new.txt").exists());
    assert!(f.store.list(None).is_empty());
    // Looked at again, with archiving: it goes, the file kept in the archive.
    let preview = inspect(f.repo.path(), &target)?;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    let Outcome::Removed {
        archive: Some(listed),
        ..
    } = outcome
    else {
        return Err(format!("not removed: {outcome:?}"));
    };
    assert_eq!(listed.manifest.files.len(), 1);
    assert_eq!(listed.manifest.files[0].path, "new.txt");
    assert!(!wt.exists());
    Ok(())
}

#[test]
fn excluded_untracked_files_keep_it_even_when_archiving() -> Result<(), String> {
    let f = Fixture::new("remove excluded")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "notes.txt", b"text\n")?;
    write(&wt, "blob.bin", b"\0\0\0")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    assert_eq!(outcome, Outcome::Kept(vec![Blocker::NotArchivable(1)]));
    assert!(wt.join("blob.bin").exists() && wt.join("notes.txt").exists());
    assert!(f.store.list(None).is_empty());
    Ok(())
}

#[test]
fn a_full_archive_store_keeps_the_worktree_and_a_retry_works() -> Result<(), String> {
    let mut f = Fixture::new("remove quota")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, UNICODE, &b"line\n".repeat(200))?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    f.store.quota = 16;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    assert!(
        matches!(&outcome, Outcome::Failed(e) if e.contains("limit")),
        "{outcome:?}"
    );
    assert!(wt.join(UNICODE).exists());
    assert!(f.store.list(None).is_empty());
    assert_eq!(
        f.store.usage(),
        0,
        "a refused archive leaves nothing behind"
    );
    f.store.quota = 64 << 20;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    assert!(
        matches!(
            outcome,
            Outcome::Removed {
                archive: Some(_),
                ..
            }
        ),
        "{outcome:?}"
    );
    assert!(!wt.exists());
    Ok(())
}

/// Git refuses to remove a worktree with a submodule checked out in it: the untracked files
/// archived and taken out just before are put back, and the archive goes.
#[test]
fn files_come_back_when_git_refuses_and_a_retry_works() -> Result<(), String> {
    let f = Fixture::new("remove refused")?;
    let sub = crate::git::stack::test_repo::TestRepo::new("wt cleanup submodule")?;
    let url = sub.path().to_string_lossy().into_owned();
    let file = ["-c", "protocol.file.allow=always"];
    f.repo
        .git(&[file[0], file[1], "submodule", "--quiet", "add", &url, "sub"])?;
    f.repo.git(&["commit", "-qm", "submodule"])?;
    let wt = f.merged_worktree("done")?;
    f.git(
        &wt,
        &[file[0], file[1], "submodule", "--quiet", "update", "--init"],
    )?;
    write(&wt, UNICODE, b"keep me\n")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert_eq!(preview.blockers_with(true), []);
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    assert_eq!(
        std::fs::read(wt.join(UNICODE)).ok(),
        Some(b"keep me\n".to_vec())
    );
    assert!(
        f.store.list(None).is_empty(),
        "the archive is only a second copy now"
    );
    // Retried once the submodule is gone (git keeps refusing while the worktree's own
    // `modules` folder is there).
    f.git(&wt, &["submodule", "--quiet", "deinit", "-f", "sub"])?;
    let dir = f.git(&wt, &["rev-parse", "--absolute-git-dir"])?;
    std::fs::remove_dir_all(std::path::Path::new(dir.trim()).join("modules"))
        .map_err(|e| e.to_string())?;
    let preview = inspect(f.repo.path(), &target)?;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store));
    assert!(
        matches!(
            outcome,
            Outcome::Removed {
                archive: Some(_),
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(f.store.list(None).len(), 1);
    Ok(())
}
