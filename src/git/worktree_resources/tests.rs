//! Real-Git inventory and conservative preview policy regression tests.
use super::*;
use crate::git::fixture::Repo;

#[test]
fn linked_worktree_inventory_is_read_only_and_preserves_ignored_secrets() -> Result<(), String> {
    let repo = Repo::new("resources-inventory")?;
    repo.commit(
        "base",
        &[
            ("Cargo.toml", Some("[package]\nname='test'\n")),
            (".gitignore", Some("target/\n.env\n")),
        ],
    )?;
    repo.git(&["branch", "topic"])?;
    let linked = repo.path().join("linked");
    repo.git(&["worktree", "add", linked.to_str().ok_or("path")?, "topic"])?;
    std::fs::create_dir_all(linked.join("target/debug/deps")).map_err(|e| e.to_string())?;
    for (path, bytes) in [
        ("target/.rustc_info.json", "{}"),
        ("target/debug/deps/output.rlib", "artifact"),
        (".env", "secret"),
        ("note", "keep"),
    ] {
        std::fs::write(linked.join(path), bytes).map_err(|e| e.to_string())?;
    }
    let inventory = collect(repo.path(), &linked, 7, Some("terminal".into()), None)?;
    assert_eq!(inventory.generation, 7);
    assert_eq!(inventory.untracked, 1);
    #[cfg(target_os = "linux")]
    {
        assert_eq!(inventory.measurements[Category::Ignored.index()].bytes, 8);
        assert_eq!(inventory.measurements[Category::Build.index()].bytes, 8);
        assert_eq!(inventory.candidates, vec![linked.join("target")]);
    }
    let plans = previews(&inventory);
    assert_eq!(plans[1].reclaim_now, Some(0));
    assert!(plans.iter().all(|p| !p.blockers.is_empty()));
    assert_eq!(
        std::fs::read_to_string(linked.join(".env")).map_err(|e| e.to_string())?,
        "secret"
    );
    assert!(collect(repo.path(), &repo.path().join("arbitrary"), 8, None, None).is_err());
    Ok(())
}

#[test]
fn classification_never_promotes_tracked_or_nonignored_files() {
    let mut members = classify::Membership::default();
    members.tracked.insert("target/secret.env".into());
    members.untracked.insert("target/notes".into());
    assert_eq!(
        members.category(std::path::Path::new("target/secret.env"), true),
        Category::Tracked
    );
    assert_eq!(
        members.category(std::path::Path::new("target/notes"), true),
        Category::Untracked
    );
    assert!(!classify::artifact(std::path::Path::new("target/.env")));
    assert!(classify::Membership::insert(b"../outside\0").is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn bounded_enumeration_and_cancellation_are_partial() -> Result<(), String> {
    let repo = Repo::new("resources-budget")?;
    repo.commit("base", &[("a", Some("a"))])?;
    let mut found = collect(repo.path(), repo.path(), 1, None, None)?;
    let membership = classify::Membership::default();
    for n in 0..100 {
        std::fs::write(repo.path().join(format!("file-{n}")), "x").map_err(|e| e.to_string())?;
    }
    walk::measure_with(&mut found, &membership, 3, &|| false)?;
    assert!(
        found
            .measurements
            .iter()
            .all(|m| matches!(m.status, Completeness::Partial(_)))
    );
    walk::measure_with(&mut found, &membership, 1000, &|| true)?;
    assert!(
        found
            .measurements
            .iter()
            .all(|m| m.status == Completeness::Partial("resources-cancelled".into()))
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn symlink_boundaries_never_measure_the_target() -> Result<(), String> {
    let repo = Repo::new("resources-links")?;
    repo.commit("base", &[("a", Some("a"))])?;
    let outside = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::write(outside.path().join("secret"), "keep").map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(outside.path(), repo.path().join("link"))
        .map_err(|e| e.to_string())?;
    let found = collect(repo.path(), repo.path(), 1, None, None)?;
    assert!(!found.complete());
    assert!(
        previews(&found)
            .iter()
            .all(|p| p.blockers.contains(&"resources-incomplete".into()))
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn content_change_invalidates_observation_and_ambiguous_target_stays_blocked() -> Result<(), String>
{
    let repo = Repo::new("resources-stale")?;
    repo.commit(
        "base",
        &[
            ("Cargo.toml", Some("[package]\n")),
            (".gitignore", Some("target/\n")),
        ],
    )?;
    std::fs::create_dir(repo.path().join("target")).map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("target/.rustc_info.json"), "{}").map_err(|e| e.to_string())?;
    let before = collect(repo.path(), repo.path(), 1, None, None)?;
    std::fs::write(repo.path().join("target/.env"), "precious credentials")
        .map_err(|e| e.to_string())?;
    let after = collect(repo.path(), repo.path(), 2, None, None)?;
    assert_ne!(before.fingerprint, after.fingerprint);
    assert!(after.candidates.is_empty());
    assert!(after.blockers.contains(&"resources-ambiguous-build".into()));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn nested_repository_stops_measurement() -> Result<(), String> {
    let repo = Repo::new("resources-nested")?;
    repo.commit("base", &[("a", Some("a"))])?;
    let nested = repo.path().join("nested");
    std::fs::create_dir_all(nested.join(".git")).map_err(|e| e.to_string())?;
    std::fs::write(nested.join("secret"), "keep").map_err(|e| e.to_string())?;
    let found = collect(repo.path(), repo.path(), 1, None, None)?;
    assert!(!found.complete());
    std::fs::remove_dir(nested.join(".git")).map_err(|e| e.to_string())?;
    repo.git(&["init", "--bare", nested.to_str().ok_or("path")?])?;
    let bare_nested = collect(repo.path(), repo.path(), 2, None, None)?;
    assert!(!bare_nested.complete());
    Ok(())
}

#[test]
fn detached_missing_upstream_and_rewritten_remote_refs_remain_protected() -> Result<(), String> {
    let repo = Repo::new("resources-publication")?;
    let base = repo.commit("base", &[("a", Some("a"))])?;
    repo.git(&["update-ref", "refs/remotes/origin/topic", &base])?;
    repo.git(&["checkout", "--detach"])?;
    let detached = collect(repo.path(), repo.path(), 1, None, None)?;
    assert!(detached.branch.is_none());
    assert!(
        crate::git::worktree_cleanup::resource_blockers(
            repo.path(),
            repo.path(),
            None,
            &detached.head,
            None
        )
        .contains(&crate::git::worktree_cleanup::Blocker::NotMerged)
    );
    repo.commit("unpublished", &[("b", Some("b"))])?;
    let unpublished = collect(repo.path(), repo.path(), 2, None, None)?;
    assert!(
        unpublished
            .blockers
            .contains(&"resources-unpublished".into())
    );
    repo.git(&["update-ref", "-d", "refs/remotes/origin/topic"])?;
    let missing = collect(repo.path(), repo.path(), 3, None, None)?;
    assert!(missing.blockers.contains(&"resources-unpublished".into()));
    Ok(())
}

#[test]
fn missing_linked_worktree_keeps_registration_but_not_zero_measurements() -> Result<(), String> {
    let repo = Repo::new("resources-missing")?;
    repo.commit("base", &[("a", Some("a"))])?;
    repo.git(&["branch", "topic"])?;
    let linked = repo.path().join("linked");
    repo.git(&["worktree", "add", linked.to_str().ok_or("path")?, "topic"])?;
    std::fs::remove_dir_all(&linked).map_err(|e| e.to_string())?;
    let found = collect(repo.path(), &linked, 1, None, None)?;
    assert_eq!(found.root, linked);
    assert!(
        found
            .measurements
            .iter()
            .all(|m| matches!(m.status, Completeness::Unavailable(_)))
    );
    assert_eq!(previews(&found)[1].potential_purge, None);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn cargo_layout_does_not_bless_secrets_or_user_supplied_libraries() -> Result<(), String> {
    let repo = Repo::new("resources-cargo-layout")?;
    repo.commit(
        "base",
        &[
            ("Cargo.toml", Some("[package]\n")),
            (".gitignore", Some("target/\n")),
        ],
    )?;
    std::fs::create_dir_all(repo.path().join("target/release/deps")).map_err(|e| e.to_string())?;
    for (name, content) in [
        ("target/.rustc_info.json", "{}"),
        ("target/.env", "secret"),
        ("target/release/libuser.a", "user"),
        ("target/release/deps/generated.rlib", "artifact"),
    ] {
        std::fs::write(repo.path().join(name), content).map_err(|e| e.to_string())?;
    }
    let found = collect(repo.path(), repo.path(), 1, None, None)?;
    assert_eq!(found.measurements[Category::Build.index()].bytes, 8);
    assert_eq!(found.measurements[Category::Ignored.index()].bytes, 12);
    assert!(found.candidates.is_empty());
    assert!(found.uncertain.contains(&"target/.env".into()));
    assert!(found.uncertain.contains(&"target/release/libuser.a".into()));
    assert!(found.blockers.contains(&"resources-ambiguous-build".into()));
    Ok(())
}

#[test]
fn stale_non_linux_capability_stays_unknown_instead_of_zero_partial() -> Result<(), String> {
    let repo = Repo::new("resources-stale-capability")?;
    repo.commit("base", &[("a", Some("a"))])?;
    let mut found = collect(repo.path(), repo.path(), 1, None, None)?;
    found.measurements[0].status = Completeness::Unsupported;
    found.measurements[1].status = Completeness::Unavailable("permission denied".into());
    found.mark_stale();
    assert_eq!(found.measurements[0].status, Completeness::Unsupported);
    assert_eq!(
        found.measurements[1].status,
        Completeness::Unavailable("permission denied".into())
    );
    assert!(found.fingerprint.is_empty());
    assert!(!found.complete());
    assert_eq!(previews(&found)[1].potential_purge, None);
    Ok(())
}
