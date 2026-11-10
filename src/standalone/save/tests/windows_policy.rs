#[cfg(windows)]
#[test]
fn native_windows_dacl_only_tightening_at_save_boundaries_never_reports_success()
-> Result<(), String> {
    let program = PathBuf::from(std::env::var_os("SystemRoot").ok_or("No SystemRoot")?)
        .join("System32/icacls.exe");
    if !program.is_absolute() {
        return Err("Fixture requires absolute SystemRoot".into());
    }
    let change = |path: &Path, grant: bool| -> Result<(), String> {
        let command = super::super::windows_metadata::fixture_acl_command(&program, path, grant);
        let out = crate::processes::run(
            command,
            crate::processes::Options {
                timeout: Some(std::time::Duration::from_secs(20)),
                output_limit: 2 * 1024 * 1024,
                stderr_limit: Some(4096),
                ..Default::default()
            },
        )?
        .complete()?;
        if !out.status.success() {
            return Err(format!(
                "Owned DACL fixture failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    };
    let temp = crate::git::file_comparison::Scratch::new()?;
    for phase in [
        Phase::BeforeMove,
        Phase::AfterMove,
        Phase::BeforeInstall,
        Phase::AfterInstall,
    ] {
        let path = temp.0.join(format!("dacl-{phase:?}"));
        write(&path, b"original")?;
        change(&path, true)?;
        let output = Output::capture(&path, &[])?;
        let original = output.original.as_ref().ok_or("No output")?;
        let identity = original.handle.as_ref().ok_or("No output identity")?;
        let original_stamp = original.stamp.clone();
        let mut hook_error = None;
        let result = output.save_with(b"candidate", &[], &mut |at| {
            if at == phase {
                let target = if phase == Phase::BeforeMove {
                    Ok(path.clone())
                } else {
                    std::fs::read_dir(&temp.0)
                        .map_err(|e| e.to_string())
                        .and_then(|entries| {
                            entries
                                .filter_map(Result::ok)
                                .map(|e| e.path().join("previous"))
                                .find(|p| {
                                    same_file::Handle::from_path(p)
                                        .is_ok_and(|h| &h == identity.as_ref())
                                })
                                .ok_or("No displaced source".into())
                        })
                };
                if let Err(e) = target.and_then(|target| change(&target, false)) {
                    hook_error = Some(e);
                }
            }
        });
        if let Some(e) = hook_error {
            return Err(e);
        }
        assert!(
            result.is_err(),
            "{phase:?} must reject DACL-only tightening"
        );
        assert_eq!(
            Some(super::super::files::Stamp::new(
                &identity.as_file().metadata().map_err(|e| e.to_string())?
            )),
            original_stamp,
            "DACL fixture must preserve ordinary guarded file fields"
        );
        if phase != Phase::AfterInstall {
            assert_eq!(read(&path)?, b"original");
        }
    }
    Ok(())
}
#[cfg(windows)]
#[test]
#[ignore = "requires an owned Windows fixture session with SeSecurityPrivilege; run explicitly after recording gates"]
fn native_windows_label_and_audit_only_drift_are_refused_and_full_copy_is_equal()
-> Result<(), String> {
    let root = PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot unavailable")?);
    if !root.is_absolute() {
        return Err("Absolute SystemRoot required".into());
    }
    let temp = crate::git::file_comparison::Scratch::new()?;
    for audit in [false, true] {
        for phase in [
            Phase::BeforeMove,
            Phase::AfterMove,
            Phase::BeforeInstall,
            Phase::AfterInstall,
        ] {
            let path = temp.0.join(format!("full-{audit}-{phase:?}"));
            write(&path, b"original")?;
            let output = Output::capture(&path, &[])?;
            let original = output.original.as_ref().ok_or("original")?;
            let identity = original.handle.as_ref().ok_or("identity")?;
            let before = super::super::windows_metadata::snapshot(&path)?;
            let mut hook_error = None;
            let result = output.save_with(b"candidate", &[], &mut |at| {
                if at != phase {
                    return;
                }
                let target = if phase == Phase::BeforeMove {
                    Ok(path.clone())
                } else {
                    std::fs::read_dir(&temp.0)
                        .map_err(|e| e.to_string())
                        .and_then(|entries| {
                            entries
                                .filter_map(Result::ok)
                                .map(|e| e.path().join("previous"))
                                .find(|p| {
                                    same_file::Handle::from_path(p)
                                        .is_ok_and(|h| &h == identity.as_ref())
                                })
                                .ok_or("No displaced original".into())
                        })
                };
                let changed = target.and_then(|target| {
                    let program = if audit {
                        root.join("System32/WindowsPowerShell/v1.0/powershell.exe")
                    } else {
                        root.join("System32/icacls.exe")
                    };
                    let command = super::super::windows_metadata::fixture_policy_command(
                        &program, &target, audit,
                    );
                    let out = crate::processes::run(
                        command,
                        crate::processes::Options {
                            timeout: Some(std::time::Duration::from_secs(20)),
                            ..Default::default()
                        },
                    )?
                    .complete()?;
                    if !out.status.success() {
                        return Err(format!(
                            "Owned full-policy fixture failed: {}",
                            String::from_utf8_lossy(&out.stderr)
                        ));
                    }
                    if super::super::windows_metadata::snapshot(&target)? == before {
                        return Err("Native label/audit did not change".into());
                    }
                    Ok(())
                });
                if let Err(e) = changed {
                    hook_error = Some(e);
                }
            });
            if let Some(e) = hook_error {
                return Err(e);
            }
            assert!(
                result.is_err(),
                "{audit} {phase:?} must refuse full-policy drift"
            );
            if phase != Phase::AfterInstall {
                assert_eq!(read(&path)?, b"original");
            }
        }
        let source = temp.0.join(format!("full-copy-{audit}"));
        write(&source, b"original")?;
        let program = if audit {
            root.join("System32/WindowsPowerShell/v1.0/powershell.exe")
        } else {
            root.join("System32/icacls.exe")
        };
        let out = crate::processes::run(
            super::super::windows_metadata::fixture_policy_command(&program, &source, audit),
            crate::processes::Options::default(),
        )?
        .complete()?;
        assert!(out.status.success());
        let before = super::super::windows_metadata::snapshot(&source)?;
        let saved = Output::capture(&source, &[])?.save(b"new", &[])?;
        assert_eq!(super::super::windows_metadata::snapshot(&source)?, before);
        assert!(saved.previous.is_some());
    }
    Ok(())
}
