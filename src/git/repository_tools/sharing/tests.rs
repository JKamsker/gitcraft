use super::*;
use crate::git::{self, fixture::Repo};
fn run(path: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(path), args)
}
fn runner<'a>(repo: &'a Path, control: &'a Arc<git::OpControl>) -> Runner<'a> {
    Runner {
        repo,
        control,
        credentials: None,
    }
}
#[test]
fn independent_pack_preserves_borrowed_private_root_reflog_index_and_original_objects()
-> Result<(), String> {
    let donor = Repo::new("independent-donor")?;
    let first = donor.commit("first", &[("file", Some("first\n"))])?;
    let second = donor.commit("second", &[("file", Some("second\n"))])?;
    donor.git(&["checkout", "--orphan", "orphan"])?;
    let orphan = donor.commit("orphan", &[("only", Some("orphan\n"))])?;
    donor.git(&["checkout", "main"])?;
    let private = git::run_input(
        Some(donor.path()),
        &["hash-object", "-w", "--stdin"],
        "private borrowed object",
    )?
    .trim()
    .to_owned();
    let indexed = git::run_input(
        Some(donor.path()),
        &["hash-object", "-w", "--stdin"],
        "indexed borrowed object",
    )?
    .trim()
    .to_owned();
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let clone = root.path().join("borrower");
    run(
        root.path(),
        &[
            "clone",
            "--shared",
            donor.path().to_str().ok_or("donor path")?,
            clone.to_str().ok_or("clone path")?,
        ],
    )?;
    run(&clone, &["reset", "--hard", &first])?;
    run(&clone, &["update-ref", "refs/gitcraft/private", &private])?;
    run(
        &clone,
        &["update-ref", &format!("refs/replace/{orphan}"), &first],
    )?;
    run(&clone, &["update-ref", "APP_ROOT", &private])?;
    run(
        &clone,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{indexed},indexed"),
        ],
    )?;
    let sharing = inspect(&clone)?;
    assert_eq!(sharing.topology.stores.len(), 2);
    assert!(sharing.blocked.is_none(), "{:?}", sharing.blocked);
    let inspected = object::inspect(&clone, &private, &|| true)?;
    assert!(
        inspected
            .representations
            .iter()
            .any(|r| r.path.starts_with(donor.path()))
    );
    let control = git::OpControl::new(|| {});
    let message = independent(
        &runner(&clone, &control),
        sharing.plan.as_ref().ok_or("no plan")?,
    )?;
    assert!(message.contains("independently verified"));
    assert!(!clone.join(".git/objects/info/alternates").exists());
    assert_eq!(
        std::fs::read_dir(clone.join(".git/objects/info"))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with("alternates.gitcraft-backup-"))
            .count(),
        1
    );
    std::fs::rename(donor.path(), root.path().join("donor unavailable"))
        .map_err(|e| e.to_string())?;
    for oid in [&first, &second, &orphan, &private, &indexed] {
        run(&clone, &["--no-replace-objects", "cat-file", "-e", oid])?;
    }
    run(&clone, &["--no-replace-objects", "fsck", "--full"])?;
    assert_eq!(inspect(&clone)?.topology.stores.len(), 1);
    Ok(())
}
#[test]
fn missing_donor_and_stale_alternate_are_diagnosed_without_detaching() -> Result<(), String> {
    let donor = Repo::new("independent-stale")?;
    donor.commit("first", &[("file", Some("first"))])?;
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let clone = root.path().join("clone");
    run(
        root.path(),
        &[
            "clone",
            "--shared",
            donor.path().to_str().ok_or("donor path")?,
            clone.to_str().ok_or("clone path")?,
        ],
    )?;
    run(&clone, &["config", "remote.origin.promisor", "FALSE"])?;
    assert!(!inspect(&clone)?.topology.promisor);
    run(&clone, &["config", "remote.origin.promisor", "TRUE"])?;
    let partial = inspect(&clone)?;
    assert!(partial.topology.promisor);
    assert!(partial.plan.is_none());
    run(&clone, &["config", "--unset", "remote.origin.promisor"])?;
    let model = inspect(&clone)?;
    let alt = clone.join(".git/objects/info/alternates");
    let old = std::fs::read(&alt).map_err(|e| e.to_string())?;
    let mut changed = old.clone();
    changed.extend_from_slice(b"\n");
    std::fs::write(&alt, &changed).map_err(|e| e.to_string())?;
    let control = git::OpControl::new(|| {});
    assert!(
        independent(
            &runner(&clone, &control),
            model.plan.as_ref().ok_or("no plan")?
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&alt).map_err(|e| e.to_string())?, changed);
    std::fs::write(&alt, old).map_err(|e| e.to_string())?;
    std::fs::rename(donor.path(), root.path().join("missing")).map_err(|e| e.to_string())?;
    let broken = inspect(&clone)?;
    assert!(broken.topology.stores.iter().any(|s| !s.available));
    assert!(broken.plan.is_none());
    assert!(alt.exists());
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlink_and_hardlink_metadata_cannot_be_detached() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let donor = Repo::new("independent-link-safety")?;
    donor.commit("first", &[("file", Some("first"))])?;
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let clone = root.path().join("clone");
    run(
        root.path(),
        &[
            "clone",
            "--shared",
            donor.path().to_str().ok_or("donor path")?,
            clone.to_str().ok_or("clone path")?,
        ],
    )?;
    let alt = clone.join(".git/objects/info/alternates");
    let saved = root.path().join("saved alternate");
    std::fs::rename(&alt, &saved).map_err(|e| e.to_string())?;
    symlink(&saved, &alt).map_err(|e| e.to_string())?;
    let linked = inspect(&clone)?;
    assert!(linked.plan.is_none());
    std::fs::remove_file(&alt).map_err(|e| e.to_string())?;
    std::fs::hard_link(&saved, &alt).map_err(|e| e.to_string())?;
    let linked = inspect(&clone)?;
    assert!(
        linked
            .blocked
            .as_ref()
            .is_some_and(|s| s.contains("hard-linked"))
    );
    assert!(saved.exists());
    Ok(())
}

// Owned-Git regression compiled, held from execution with the runtime fixtures.
#[test]
fn diamond_donors_remain_visible_and_allow_independence() -> Result<(), String> {
    let a = Repo::new("diamond-a")?;
    a.commit("a", &[])?;
    let b = Repo::new("diamond-b")?;
    b.commit("b", &[])?;
    let c = Repo::new("diamond-c")?;
    c.commit("c", &[])?;
    let d = Repo::new("diamond-d")?;
    let borrowed = d.commit("borrowed", &[])?;
    for (repo, donors) in [
        (a.path(), vec![b.path(), c.path()]),
        (b.path(), vec![d.path()]),
        (c.path(), vec![d.path()]),
    ] {
        let info = repo.join(".git/objects/info");
        std::fs::create_dir_all(&info).map_err(|e| e.to_string())?;
        let bytes = donors
            .iter()
            .map(|p| format!("{}\n", p.join(".git/objects").display()))
            .collect::<String>();
        std::fs::write(info.join("alternates"), bytes).map_err(|e| e.to_string())?;
    }
    a.git(&["update-ref", "refs/gitcraft/borrowed", &borrowed])?;
    let model = inspect(a.path())?;
    assert_eq!(model.topology.stores.len(), 4);
    assert_eq!(model.topology.edges.len(), 4);
    assert!(model.topology.warnings.is_empty());
    assert!(model.blocked.is_none(), "{:?}", model.blocked);
    let control = git::OpControl::new(|| {});
    independent(
        &runner(a.path(), &control),
        model.plan.as_ref().ok_or("No plan")?,
    )?;
    assert_eq!(inspect(a.path())?.topology.stores.len(), 1);
    a.git(&["cat-file", "-e", &borrowed])?;
    Ok(())
}
