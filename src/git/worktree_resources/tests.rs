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
    std::fs::create_dir(linked.join("target")).map_err(|e| e.to_string())?;
    for (path, bytes) in [
        ("target/.rustc_info.json", "{}"),
        ("target/output.rlib", "artifact"),
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
        assert_eq!(inventory.measurements[Category::Ignored.index()].bytes, 6);
        assert_eq!(inventory.measurements[Category::Build.index()].bytes, 10);
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
    Ok(())
}
