use super::*;
#[test]
fn inactive_missing_and_nested_include_targets_are_dependencies() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("staged-watch-includes")?;
    let outside = tempfile::tempdir().map_err(|e| e.to_string())?;
    let nested = outside.path().join("nested config");
    let missing = outside.path().join("absent").join("config");
    std::fs::write(&nested, "[include]\n path = absent/config\n").map_err(|e| e.to_string())?;
    repo.git(&[
        "config",
        "includeIf.onbranch:never-active.path",
        &nested.to_string_lossy(),
    ])?;
    let files = paths(repo.path());
    assert!(contains(&files, &nested), "inactive includes need watches");
    assert!(
        contains(&files, &missing),
        "missing nested includes need watches"
    );
    assert!(contains(&parents(&files), outside.path()));
    assert!(changed(&files, &outside.path().join("absent")));
    assert!(!changed(&files, &outside.path().join("unrelated")));
    std::fs::create_dir(missing.parent().ok_or("missing parent")?).map_err(|e| e.to_string())?;
    assert!(contains(
        &parents(&paths(repo.path())),
        missing.parent().ok_or("missing parent")?
    ));
    Ok(())
}
#[test]
fn rediscovery_follows_new_includes_and_condition_activation() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("staged-watch-new-include")?;
    repo.commit("base", &[])?;
    let outside = tempfile::tempdir().map_err(|e| e.to_string())?;
    let first = outside.path().join("first");
    let second = outside.path().join("second");
    assert!(!contains(&paths(repo.path()), &first));
    repo.git(&[
        "config",
        "includeIf.onbranch:future.path",
        &first.to_string_lossy(),
    ])?;
    assert!(contains(&paths(repo.path()), &first));
    std::fs::write(&first, "[include]\n path = second\n").map_err(|e| e.to_string())?;
    assert!(contains(&paths(repo.path()), &second));
    repo.git(&["checkout", "-b", "future"])?;
    assert!(contains(&paths(repo.path()), &second));
    assert!(changed(&paths(repo.path()), &second));
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_config_and_nested_missing_parents_keep_consistent_keys() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("staged-watch-symlink-config")?;
    let outside = tempfile::tempdir().map_err(|e| e.to_string())?;
    let real = outside.path().join("real");
    let alias = outside.path().join("alias");
    std::fs::create_dir(&real).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&real, &alias).map_err(|e| e.to_string())?;
    let target = real.join("gitconfig");
    std::fs::write(&target, "[include]\npath = missing/nested/config\n")
        .map_err(|e| e.to_string())?;
    let link = outside.path().join("config-link");
    std::os::unix::fs::symlink(&target, &link).map_err(|e| e.to_string())?;
    repo.git(&["config", "include.path", &link.to_string_lossy()])?;
    repo.git(&[
        "config",
        "includeIf.onbranch:inactive.path",
        &alias.join("absent/nested/config").to_string_lossy(),
    ])?;
    let files = paths(repo.path());
    assert!(contains(&files, &link), "keep link replacement events");
    assert!(contains(&files, &target), "watch actual config contents");
    assert!(contains(&parents(&files), &real));
    assert!(changed(&files, &target));
    let missing = alias.join("absent/nested/config");
    assert_eq!(key(&missing), key(&real.join("absent/nested/config")));
    assert!(changed(&files, &real.join("absent")));
    assert!(covers(&missing, &real));
    std::fs::create_dir(real.join("absent")).map_err(|e| e.to_string())?;
    assert!(changed(&files, &real.join("absent")));
    assert_eq!(key(&missing), key(&real.join("absent/nested/config")));
    Ok(())
}
