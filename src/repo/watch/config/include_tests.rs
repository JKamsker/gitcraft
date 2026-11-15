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
    assert!(parents(&files).contains(&outside.path().to_path_buf()));
    assert!(changed(&files, &outside.path().join("absent")));
    assert!(!changed(&files, &outside.path().join("unrelated")));
    std::fs::create_dir(missing.parent().ok_or("missing parent")?).map_err(|e| e.to_string())?;
    assert!(
        parents(&paths(repo.path())).contains(&missing.parent().ok_or("missing parent")?.into())
    );
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
