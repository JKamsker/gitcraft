// Ssh options. Included to preserve libtest shard names.
#[test]
fn profile_openssh_options_refuse_plink_dialects_before_any_probe() {
    for args in [
        vec!["-P", "2222", "git@fixture"],
        vec!["-P2222", "git@fixture"],
        vec!["-vP2222", "git@fixture"],
        vec!["-batch", "git@fixture"],
        vec!["-ssh", "git@fixture"],
        vec!["--putty", "git@fixture"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(validate_ssh_options(&args).is_err_and(|e| e.contains("incompatible SSH options")));
    }
    for args in [
        vec![
            "-G",
            "-o",
            "SendEnv=GIT_PROTOCOL",
            "-p",
            "2222",
            "git@fixture",
            "git-upload-pack '/path'",
        ],
        vec![
            "-vv",
            "-4",
            "-F",
            "/fixture/-P-config",
            "-o",
            "ProxyCommand=tool -P 2222",
            "git@fixture",
        ],
        vec!["--", "git@fixture", "-P remote-command-data"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_ok()
        );
    }
}

#[test]
fn profile_openssh_checks_options_after_the_hostname_until_the_command_boundary() {
    for args in [
        vec!["git@fixture", "-P", "2222"],
        vec!["-vv", "git@fixture", "-vP2222"],
        vec!["git@fixture", "-batch"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
        );
    }
    for args in [
        vec!["git@fixture", "-p", "2222", "git-upload-pack '/x'"],
        vec!["--", "git@fixture", "-P2222"],
        vec!["git@fixture", "--", "-P2222"],
        vec!["git@fixture", "git-upload-pack /x", "-P2222"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_ok()
        );
    }
}

#[test]
fn scoped_workers_keep_the_captured_profile_and_cancellation_even_after_context_changes()
-> Result<(), String> {
    let parent = crate::tasks::Cancellation::default();
    let a = Context {
        error: Some("captured profile".into()),
        ..Default::default()
    };
    let b = Context {
        error: Some("later profile".into()),
        ..Default::default()
    };
    crate::tasks::with_cancellation(parent.clone(), || {
        a.run(|| {
            let work = inherit(|| (current(), crate::tasks::cancellation().is_cancelled()));
            parent.cancel();
            b.run(|| {
                std::thread::scope(|scope| {
                    let (context, cancelled) = scope
                        .spawn(work)
                        .join()
                        .map_err(|_| "synthetic worker panicked".to_owned())?;
                    assert_eq!(context, a);
                    assert!(cancelled);
                    assert_eq!(current(), b);
                    Ok::<_, String>(())
                })
            })
        })
    })?;
    assert!(
        !crate::tasks::cancellation().is_cancelled(),
        "the caller's prior lifetime is restored"
    );
    Ok(())
}

#[test]
fn selected_key_transport_refuses_session_and_identity_substitution() -> Result<(), String> {
    let command = ssh_command("/fixture/selected-key");
    let args = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for policy in [
        "ControlPath=none",
        "ControlMaster=no",
        "ControlPersist=no",
        "IdentitiesOnly=yes",
        "IdentityAgent=none",
        "CertificateFile=none",
        "PKCS11Provider=none",
        "ForwardAgent=no",
        "PreferredAuthentications=publickey",
    ] {
        assert!(args.iter().any(|a| a == policy), "{policy}");
    }
    for args in [
        ["-S", "/fixture/personal-master", "git@fixture"],
        ["-I", "/fixture/provider", "git@fixture"],
        ["-i", "/fixture/personal-key", "git@fixture"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
        );
    }
    assert!(
        validate_ssh_identities(
            "/fixture/selected-key",
            "identityfile /fixture/selected-key\ncertificatefile /fixture/other-account-cert.pub\n",
            &[]
        )
        .is_err()
    );
    assert!(
        validate_ssh_identities(
            "/fixture/selected-key",
            "identityfile /fixture/selected-key\ncertificatefile none\n",
            &[]
        )
        .is_ok()
    );
    Ok(())
}
