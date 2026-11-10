#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn acl_only_tightening_during_each_save_boundary_refuses_stale_policy_and_retains_recovery()
-> Result<(), String> {
    use rustix::fs::{XattrFlags, fgetxattr, fsetxattr};
    use std::os::unix::fs::MetadataExt;
    fn acl(uid: u32, permission: u16) -> Vec<u8> {
        let mut bytes = 2u32.to_le_bytes().to_vec();
        for (tag, perm, id) in [
            (1u16, 6u16, u32::MAX),
            (2, permission, uid),
            (4, 4, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&perm.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        bytes
    }
    let temp = crate::git::file_comparison::Scratch::new()?;
    for phase in [
        Phase::BeforeMove,
        Phase::AfterMove,
        Phase::BeforeInstall,
        Phase::AfterInstall,
    ] {
        let path = temp.0.join(format!("acl-{phase:?}"));
        write(&path, b"original")?;
        let source = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let uid = source
            .metadata()
            .map_err(|e| e.to_string())?
            .uid()
            .checked_add(1)
            .ok_or("No fixture uid")?;
        let before = acl(uid, 4);
        let tightened = acl(uid, 0);
        fsetxattr(
            &source,
            "system.posix_acl_access",
            &before,
            XattrFlags::empty(),
        )
        .map_err(|e| e.to_string())?;
        let output = Output::capture(&path, &[])?;
        let original_stamp = output.original.as_ref().ok_or("No output")?.stamp.clone();
        let mut hook_error = None;
        let result = output.save_with(b"candidate", &[], &mut |at| {
            if at == phase
                && let Err(e) = fsetxattr(
                    &source,
                    "system.posix_acl_access",
                    &tightened,
                    XattrFlags::empty(),
                )
            {
                hook_error = Some(e.to_string());
            }
        });
        if let Some(e) = hook_error {
            return Err(e);
        }
        assert!(
            result.is_err(),
            "{phase:?} must not report a successful stale-policy save"
        );
        assert_eq!(
            Some(super::super::files::Stamp::new(
                &source.metadata().map_err(|e| e.to_string())?
            )),
            original_stamp,
            "ACL-only edit must leave the old guarded fields identical"
        );
        let mut bytes = [0; 128];
        let n = fgetxattr(&source, "system.posix_acl_access", &mut bytes[..])
            .map_err(|e| e.to_string())?;
        assert_eq!(&bytes[..n], tightened);
        let recoveries = std::fs::read_dir(&temp.0)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("previous").is_file())
            .collect::<Vec<_>>();
        assert!(
            !recoveries.is_empty(),
            "displaced bytes and tightened metadata must remain recoverable"
        );
        if phase != Phase::AfterInstall {
            assert_eq!(read(&path)?, b"original");
            let now = Output::capture(&path, &[])?;
            assert_ne!(
                now.original.as_ref().ok_or("No restored output")?.access,
                output.original.as_ref().ok_or("No source")?.access
            );
        }
    }
    Ok(())
}
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn xattr_only_drift_since_open_or_after_copy_is_not_a_successful_save() -> Result<(), String> {
    use rustix::fs::{XattrFlags, fsetxattr};
    let temp = crate::git::file_comparison::Scratch::new()?;
    for late in [false, true] {
        let path = temp.0.join(if late { "after-copy" } else { "since-open" });
        write(&path, b"original")?;
        let source = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        fsetxattr(
            &source,
            "user.gitcraft-policy",
            b"before",
            XattrFlags::empty(),
        )
        .map_err(|e| e.to_string())?;
        let output = Output::capture(&path, &[])?;
        if !late {
            fsetxattr(
                &source,
                "user.gitcraft-policy",
                b"after",
                XattrFlags::empty(),
            )
            .map_err(|e| e.to_string())?;
        }
        let mut hook_error = None;
        let result = output.save_with(b"candidate", &[], &mut |phase| {
            if late
                && phase == Phase::BeforeMove
                && let Err(e) = fsetxattr(
                    &source,
                    "user.gitcraft-policy",
                    b"after",
                    XattrFlags::empty(),
                )
            {
                hook_error = Some(e.to_string());
            }
        });
        if let Some(e) = hook_error {
            return Err(e);
        }
        assert!(result.is_err());
        assert_eq!(read(&path)?, b"original");
    }
    Ok(())
}
