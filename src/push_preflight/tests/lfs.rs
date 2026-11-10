#[test]
fn local_lfs_repair_rewrites_pointers_attributes_and_keeps_source_index_consistent()
-> Result<(), String> {
    let f = Fixture::new()?;
    std::fs::write(
        f.repo.join("large [asset].bin"),
        vec![b'x'; 6 * 1024 * 1024],
    )
    .map_err(|e| e.to_string())?;
    f.git(&["add", "large [asset].bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    let plan = f.plan(f.profile())?;
    fix(&f.repo, &plan, "lfs")?;
    let pointer = f.git(&["show", "HEAD:large [asset].bin"])?;
    assert!(git::parse_lfs_pointer(&pointer).is_some());
    let attrs = f.git(&[
        "check-attr",
        "--cached",
        "filter",
        "--",
        "large [asset].bin",
    ])?;
    assert!(attrs.ends_with(": lfs"), "{attrs}");
    assert!(f.git(&["status", "--porcelain=v1"])?.is_empty());
    assert!(
        f.plan(f.profile())?
            .findings
            .iter()
            .all(|x| x.kind != Kind::Large)
    );
    Ok(())
}

#[test]
fn repaired_lfs_objects_are_uploaded_with_the_confirmed_push_even_without_a_hook()
-> Result<(), String> {
    let f = Fixture::new()?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    let plan = f.plan(f.profile())?;
    fix(&f.repo, &plan, "lfs")?;
    let plan = f.plan(f.profile())?;
    assert_eq!(plan.lfs_objects.len(), 1);
    execute(&f.repo, &plan, None)?;
    let oid = plan.lfs_objects.first().ok_or("No LFS object")?;
    let stored = f
        .receiver
        .join("lfs/objects")
        .join(&oid[..2])
        .join(&oid[2..4])
        .join(oid);
    assert!(stored.is_file(), "{}", stored.display());
    assert_eq!(f.receiver_head()?, plan.head);
    Ok(())
}

#[test]
fn lfs_endpoint_working_changes_invalidate_approval_before_any_network_step() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.commit(
        ".lfsconfig",
        "[lfs]\nurl = https://lfs.example.org/work\n",
        "LFS endpoint",
    )?;
    let plan = f.plan(f.profile())?;
    let before = f.receiver_head()?;
    std::fs::write(
        f.repo.join(".lfsconfig"),
        "[lfs]\nurl = https://wrong.example.org/personal\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(check(&f.repo, &plan).is_err());
    assert_eq!(f.receiver_head()?, before);
    Ok(())
}

#[test]
fn lfs_upload_works_with_the_existing_local_lfs_hook_too() -> Result<(), String> {
    let f = Fixture::new()?;
    f.git(&["lfs", "install", "--local"])?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    let plan = f.plan(f.profile())?;
    fix(&f.repo, &plan, "lfs")?;
    let plan = f.plan(f.profile())?;
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, plan.head);
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejecting_pre_push_hook_preserves_receiver_refs_and_lfs_objects() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    fix(&f.repo, &f.plan(f.profile())?, "lfs")?;
    let marker = f.root.join("hook-ran");
    let hook = f.repo.join(".git/hooks/pre-push");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf 'rejected\n' > {}\nexit 1\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let plan = f.plan(f.profile())?;
    let before = f.receiver_head()?;
    let oid = plan.lfs_objects.iter().next().ok_or("No LFS object")?;
    let stored = f
        .receiver
        .join("lfs/objects")
        .join(&oid[..2])
        .join(&oid[2..4])
        .join(oid);
    assert!(
        !stored.exists(),
        "The receiver starts without this LFS object"
    );
    assert!(execute(&f.repo, &plan, None).is_err());
    assert!(marker.exists(), "The original hook must run");
    assert_eq!(f.receiver_head()?, before);
    assert!(
        !stored.exists(),
        "A hook veto must precede explicit LFS upload"
    );
    Ok(())
}
