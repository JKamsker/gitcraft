//! Only owned temporary directories are changed; no real mounts or process signals.
use crate::git::worktree_cleanup::archive::Store;
use crate::git::worktree_cleanup::tests::Fixture;
use crate::git::worktree_cleanup::{Outcome, inspect, remove};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn retained(f: &Fixture) -> std::result::Result<std::path::PathBuf, String> {
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(matches!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Removed { .. }
    ));
    Store::recoveries(f.repo.path())?
        .into_iter()
        .next()
        .map(|(path, _)| path)
        .ok_or("Missing recovery".into())
}

#[cfg(unix)]
#[test]
fn ancestor_replaced_after_validation_cannot_redirect_purge() -> Result {
    let f = Fixture::new("purge ancestor replacement")?;
    let entry = retained(&f)?;
    let root = entry.parent().ok_or("Missing parent")?;
    let parked = root.with_extension("parked");
    let name = entry.file_name().ok_or("Missing name")?;
    let outcome = super::purge_after_validation(f.repo.path(), &entry, || {
        assert!(std::fs::rename(root, &parked).is_ok());
        assert!(std::fs::create_dir(root).is_ok());
        assert!(std::fs::create_dir(root.join(name)).is_ok());
        assert!(std::fs::write(root.join(name).join("precious"), "outside recovery").is_ok());
    });
    assert!(outcome.is_err());
    assert_eq!(
        std::fs::read_to_string(entry.join("precious"))?,
        "outside recovery"
    );
    assert!(parked.join(name).join("tree/base.txt").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn entry_replaced_after_validation_preserves_both_trees() -> Result {
    let f = Fixture::new("purge entry replacement")?;
    let entry = retained(&f)?;
    let parked = entry.with_extension("parked");
    assert!(
        super::purge_after_validation(f.repo.path(), &entry, || {
            assert!(std::fs::rename(&entry, &parked).is_ok());
            assert!(std::fs::create_dir(&entry).is_ok());
            assert!(std::fs::write(entry.join("precious"), "replacement").is_ok());
        })
        .is_err()
    );
    assert!(parked.join("tree/base.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(entry.join("precious"))?,
        "replacement"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn held_ancestors_prevent_windows_replacement() -> Result {
    let f = Fixture::new("purge windows ancestry")?;
    let entry = retained(&f)?;
    let root = entry.parent().ok_or("Missing parent")?;
    super::purge_after_validation(f.repo.path(), &entry, || {
        assert!(std::fs::rename(root, root.with_extension("parked")).is_err());
        assert!(std::fs::rename(&entry, entry.with_extension("parked")).is_err());
    })?;
    assert!(!entry.exists());
    Ok(())
}
