#[test]
fn malformed_line_ids_and_submodule_boundaries_are_read_only() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "base\n")?;
    r.commit()?;
    r.write("a", "edit\n")?;
    let mut h = harness(&r, Page::Lens)?;
    settle(&mut h)?;
    let request = h
        .state()
        .0
        .file_comparison
        .as_ref()
        .ok_or("state")?
        .data_request;
    let ctx = h.ctx.clone();
    h.state_mut()
        .0
        .comparison_pick(request, 1, [usize::MAX].into(), false, &ctx);
    assert!(h.state().0.busy_op.is_none());
    assert_eq!(r.git(&["show", ":0:a"])?, "base");
    let mut part = read(&r, "a")?.boundaries[1].clone();
    part.old_mode = Some("160000".into());
    assert!(!part.writable());
    assert!(part.patch("a", Pick::Stage, &part.changed()).is_none());
    let mut snapshot = r.lens("a")?;
    snapshot.versions[0].mode = Some("160000".into());
    assert!(
        files::apply_lens_patch(&r.0, &snapshot, &part.full, false)
            .is_err_and(|e| e.contains("read-only"))
    );
    assert_eq!(r.git(&["show", ":0:a"])?, "base");
    Ok(())
}

#[test]
fn intent_to_add_and_empty_file_creation_deletion_keep_commit_semantics() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("tracked", "base\n")?;
    r.commit()?;
    r.write("new", "one\ntwo\n")?;
    r.git(&["add", "-N", "new"])?;
    let lens = read(&r, "new")?;
    assert!(lens.snapshot.versions[0].mode.is_none());
    assert!(lens.snapshot.versions[1].mode.is_none());
    let p = &lens.boundaries[1];
    let patch = p
        .patch("new", Pick::Stage, &changed(p, "one"))
        .ok_or("intent-to-add patch")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, false)?;
    assert_eq!(r.git(&["show", ":0:new"])?, "one");
    assert_eq!(
        std::fs::read(r.0.join("new")).map_err(|e| e.to_string())?,
        b"one\ntwo\n"
    );
    r.write("empty", b"")?;
    let lens = read(&r, "empty")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert!(r.git(&["ls-files", "--error-unmatch", "empty"]).is_ok());
    r.commit()?;
    std::fs::remove_file(r.0.join("empty")).map_err(|e| e.to_string())?;
    let lens = read(&r, "empty")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert!(r.git(&["ls-files", "--error-unmatch", "empty"]).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn mode_only_boundaries_and_staged_rename_content_keep_paths_and_working_bytes()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let r = Repo::new()?;
    r.write("old", "base\n")?;
    r.commit()?;
    std::fs::set_permissions(r.0.join("old"), std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let lens = read(&r, "old")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert!(
        r.git(&["ls-files", "--stage", "old"])?
            .starts_with("100755")
    );
    r.git(&["mv", "old", "new"])?;
    r.write("new", "staged\n")?;
    r.git(&["add", "new"])?;
    r.write("new", "working\n")?;
    let snapshot = files::read_lens(
        &r.0,
        &files::LensSpec {
            path: "new".into(),
            previous: Some("old".into()),
        },
        &|| true,
    )?;
    let lens = model::prepare_lens(snapshot, &|| true)?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[0].full, true)?;
    assert_eq!(r.git(&["show", ":0:new"])?, "base");
    assert!(r.git(&["ls-files", "--error-unmatch", "old"]).is_err());
    assert_eq!(
        std::fs::read(r.0.join("new")).map_err(|e| e.to_string())?,
        b"working\n"
    );
    Ok(())
}

#[test]
fn emulated_symlinks_keep_the_index_type_like_git_add_and_guard_config_changes()
-> Result<(), String> {
    let r = Repo::new()?;
    r.git(&["config", "core.symlinks", "false"])?;
    r.write("link", b"original-target")?;
    let original = r.git(&["hash-object", "--no-filters", "-w", "link"])?;
    r.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("120000,{original},link"),
    ])?;
    r.git(&["commit", "-qm", "emulated link"])?;
    let unchanged = read(&r, "link")?;
    assert_eq!(unchanged.snapshot.work_raw.mode.as_deref(), Some("100644"));
    assert!(
        unchanged
            .snapshot
            .versions
            .iter()
            .all(|v| v.mode.as_deref() == Some("120000"))
    );
    assert!(unchanged.boundaries[1].changed().is_empty());
    assert_eq!(
        unchanged.boundaries[1].old_mode,
        unchanged.boundaries[1].new_mode
    );
    r.write("link", b"new-target")?;
    r.git(&["add", "link"])?;
    let native_result = r.git(&["ls-files", "--stage", "link"])?;
    assert!(native_result.starts_with("120000"));
    r.git(&[
        "update-index",
        "--cacheinfo",
        &format!("120000,{original},link"),
    ])?;
    let lens = read(&r, "link")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert_eq!(r.git(&["ls-files", "--stage", "link"])?, native_result);
    assert_eq!(
        std::fs::read(r.0.join("link")).map_err(|e| e.to_string())?,
        b"new-target"
    );
    r.write("link", b"later-target")?;
    let lens = read(&r, "link")?;
    r.git(&["config", "core.symlinks", "true"])?;
    assert!(
        files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)
            .is_err_and(|e| e.contains("stale"))
    );
    assert_eq!(r.git(&["ls-files", "--stage", "link"])?, native_result);
    Ok(())
}
