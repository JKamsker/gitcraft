//! Metadata/configuration policy changes and native ACL preservation.
use super::*;

#[test]
fn configured_private_and_group_modes_match_native_git_and_policy_drift_is_refused()
-> Result<(), String> {
    for sharing in ["0600", "group"] {
        let (r, _, patch) = fixture()?;
        let (native, _, _) = fixture()?;
        for repo in [&r, &native] {
            repo.git(&["config", "core.sharedRepository", sharing])?;
            repo.git(&["add", "a"])?;
        }
        let expected = std::fs::metadata(native.0.join(".git/index"))
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777;
        r.write("a", "next\n")?;
        let lens = r.lens("a")?;
        let patch = patch
            .replace("-before", "-after")
            .replace("+after", "+next");
        apply(&r.0, &lens, &patch, false, &mut |_| {})?;
        let mode = std::fs::metadata(r.0.join(".git/index"))
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, expected, "{sharing}");
        assert_eq!(r.git(&["show", ":a"])?, "next");
        assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
    }
    for config in [false, true] {
        let (r, _, patch) = fixture()?;
        r.git(&["config", "core.sharedRepository", "false"])?;
        let lens = r.lens("a")?;
        let idx = r.0.join(".git/index");
        let old = bytes(&idx)?;
        let initial_mode = index_mode(&idx)?;
        let mut injected = false;
        let mut operation_hook_error = None;
        let result = apply(&r.0, &lens, &patch, false, &mut |at| {
            if at == Phase::Prepared {
                let change = (|| -> Result<(), String> {
                    if config {
                        r.git(&["config", "core.sharedRepository", "0600"])?;
                        assert_eq!(
                            r.git(&["config", "--get", "core.sharedRepository"])?,
                            "0600"
                        );
                    } else {
                        let changed_mode = initial_mode ^ 0o040;
                        assert_ne!(changed_mode, initial_mode);
                        std::fs::set_permissions(
                            &idx,
                            std::fs::Permissions::from_mode(changed_mode),
                        )
                        .map_err(|e| e.to_string())?;
                        assert_eq!(index_mode(&idx)?, changed_mode);
                    }
                    Ok(())
                })();
                injected = record(change, &mut operation_hook_error);
            }
        });
        if let Some(e) = operation_hook_error {
            return Err(e);
        }
        assert!(injected, "policy hook must run before testing refusal");
        assert!(result.is_err());
        assert_eq!(bytes(&idx)?, old);
        assert_eq!(bytes(&r.0.join("a"))?, b"after\n");
        assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
    }
    let r = Repo::new()?;
    r.git(&["config", "core.sharedRepository", "0600"])?;
    r.write("a", "new\n")?;
    let lens = read_lens(
        &r.0,
        &LensSpec {
            path: "a".into(),
            previous: None,
        },
        &|| true,
    )?;
    apply(
        &r.0,
        &lens,
        "diff --git \"a/a\" \"b/a\"\nnew file mode 100644\n--- /dev/null\n+++ \"b/a\"\n@@ -0,0 +1 @@\n+new\n",
        false,
        &mut |_| {},
    )?;
    assert_eq!(
        std::fs::metadata(r.0.join(".git/index"))
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    Ok(())
}

fn index_mode(path: &Path) -> Result<u32, String> {
    Ok(std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions()
        .mode()
        & 0o777)
}

#[test]
fn actual_mode_drift_and_same_mode_controls_cover_private_and_shared_indices() -> Result<(), String>
{
    for initial in [0o600, 0o644, 0o660] {
        for drift in [false, true] {
            let (r, _, patch) = fixture()?;
            let sharing = format!("{initial:04o}");
            r.git(&["config", "core.sharedRepository", &sharing])?;
            let idx = r.0.join(".git/index");
            std::fs::set_permissions(&idx, std::fs::Permissions::from_mode(initial))
                .map_err(|e| e.to_string())?;
            assert_eq!(index_mode(&idx)?, initial);
            let lens = r.lens("a")?;
            let original = bytes(&idx)?;
            let target = if drift { initial ^ 0o040 } else { initial };
            assert_eq!(initial != target, drift);
            let mut reached = false;
            let mut hook_error = None;
            let result = apply(&r.0, &lens, &patch, false, &mut |at| {
                if at == Phase::Prepared {
                    let change = (|| -> Result<(), String> {
                        assert_eq!(index_mode(&idx)?, initial);
                        std::fs::set_permissions(&idx, std::fs::Permissions::from_mode(target))
                            .map_err(|e| e.to_string())?;
                        assert_eq!(index_mode(&idx)?, target);
                        Ok(())
                    })();
                    reached = record(change, &mut hook_error);
                }
            });
            if let Some(error) = hook_error {
                return Err(error);
            }
            assert!(reached, "initial={initial:o} drift={drift}: {result:?}");
            if drift {
                assert!(
                    result.is_err(),
                    "real permission drift must refuse publication"
                );
                assert_eq!(bytes(&idx)?, original);
                assert_eq!(r.git(&["show", ":a"])?, "before");
            } else {
                result?;
                assert_eq!(r.git(&["show", ":a"])?, "after");
            }
            assert_eq!(index_mode(&idx)?, target);
            assert_eq!(bytes(&r.0.join("a"))?, b"after\n");
            assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn native_index_acl_is_preserved_and_acl_only_drift_cannot_succeed() -> Result<(), String> {
    let (r, _, patch) = fixture()?;
    let idx = r.0.join(".git/index");
    let acl = vec![exacl::AclEntry::allow_user(
        "nobody",
        exacl::Perm::READ,
        None,
    )];
    exacl::setfacl(&[&idx], &acl, None).map_err(|e| e.to_string())?;
    let expected =
        exacl::getfacl(&idx, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?;
    apply(&r.0, &r.lens("a")?, &patch, false, &mut |_| {})?;
    assert_eq!(
        exacl::getfacl(&idx, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?,
        expected
    );
    r.write("a", "next\n")?;
    let lens = r.lens("a")?;
    let old = bytes(&idx)?;
    let changed = vec![exacl::AclEntry::deny_user(
        "nobody",
        exacl::Perm::READ,
        None,
    )];
    let mut operation_hook_error: Option<String> = None;
    let result = apply(
        &r.0,
        &lens,
        &patch
            .replace("-before", "-after")
            .replace("+after", "+next"),
        false,
        &mut |at| {
            if at == Phase::Prepared {
                record(
                    exacl::setfacl(&[&idx], &changed, None),
                    &mut operation_hook_error,
                );
            }
        },
    );
    if let Some(e) = operation_hook_error {
        return Err(e);
    }
    assert!(result.is_err());
    assert_eq!(bytes(&idx)?, old);
    Ok(())
}
