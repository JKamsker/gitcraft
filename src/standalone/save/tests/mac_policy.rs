#[cfg(target_os = "macos")]
#[test]
fn native_mac_acl_clone_preserves_policy_and_acl_only_drift_refuses_success() -> Result<(), String>
{
    use std::os::unix::fs::MetadataExt;
    let temp = crate::git::file_comparison::Scratch::new()?;
    let path = temp.0.join("native-acl");
    write(&path, b"original")?;
    let uid = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .uid()
        .to_string();
    let initial = vec![exacl::AclEntry::allow_user(&uid, exacl::Perm::READ, None)];
    exacl::setfacl(&[&path], &initial, None).map_err(|e| e.to_string())?;
    let expected =
        exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?;
    let saved = Output::capture(&path, &[])?.save(b"new result", &[])?;
    assert_eq!(read(&path)?, b"new result");
    assert_eq!(
        exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?,
        expected
    );
    assert_eq!(
        read(&saved.previous.ok_or("No retained original")?)?,
        b"original"
    );
    for phase in [
        Phase::BeforeMove,
        Phase::AfterMove,
        Phase::BeforeInstall,
        Phase::AfterInstall,
    ] {
        write(&path, b"original")?;
        exacl::setfacl(&[&path], &initial, None).map_err(|e| e.to_string())?;
        let output = Output::capture(&path, &[])?;
        let source_metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        let changed = vec![exacl::AclEntry::allow_user(
            &uid,
            exacl::Perm::empty(),
            None,
        )];
        let mut hook_error = None;
        let result = output.save_with(b"candidate", &[], &mut |at| {
            if at == phase {
                let target = if matches!(phase, Phase::BeforeMove) {
                    Ok(path.clone())
                } else {
                    std::fs::read_dir(&temp.0)
                        .map_err(|e| e.to_string())
                        .and_then(|entries| {
                            entries
                                .filter_map(Result::ok)
                                .map(|e| e.path().join("previous"))
                                .find(|p| {
                                    std::fs::metadata(p).is_ok_and(|m| {
                                        m.dev() == source_metadata.dev()
                                            && m.ino() == source_metadata.ino()
                                    })
                                })
                                .ok_or("No displaced source".into())
                        })
                };
                match target.and_then(|target| {
                    exacl::setfacl(&[target], &changed, None).map_err(|e| e.to_string())
                }) {
                    Ok(()) => {}
                    Err(e) => hook_error = Some(e),
                }
            }
        });
        if let Some(e) = hook_error {
            return Err(e);
        }
        assert!(
            result.is_err(),
            "{phase:?} must reject descriptor-only drift"
        );
        if phase != Phase::AfterInstall {
            assert_eq!(read(&path)?, b"original");
        }
    }
    Ok(())
}
