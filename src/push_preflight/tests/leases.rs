#[test]
fn a_stale_server_lease_does_not_upload_lfs_payloads() -> Result<(), String> {
    let f = Fixture::new()?;
    f.git(&["lfs", "install", "--local"])?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    fix(&f.repo, &f.plan(f.profile())?, "lfs")?;
    let plan = f.plan(f.profile())?;
    let advanced = f.advance_receiver()?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.receiver_head()?, advanced);
    assert!(!f.receiver.join("lfs/objects").exists());
    Ok(())
}

#[test]
fn current_porcelain_push_and_skip_hook_policy_keep_lease_and_lfs_guards() -> Result<(), String> {
    let f = Fixture::new()?;
    let tip = f.commit("next", "next", "Next")?;
    let hooks = f.repo.join(".git/hooks/pre-push");
    std::fs::write(&hooks, "#!/bin/sh\nexit 42\n").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hooks, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "--porcelain".into(),
            "--no-verify".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    assert!(plan.skip_hooks);
    let args = push_args(&plan)?;
    assert!(args.iter().any(|s| s == "--porcelain"));
    assert!(!args.iter().any(|s| s == "--no-verify"));
    assert!(args.iter().any(|s| s.starts_with("--force-with-lease=")));
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, tip);
    Ok(())
}

/// Supplied argv/tracking data only: no Fixture, Git, filesystem or runtime.
#[test]
fn recording_fork_explicit_lease_is_fixed_before_and_after_approval() -> Result<(), String> {
    let destination = "refs/heads/topic";
    for width in [40, 64] {
        let expected = "a".repeat(width);
        let newer = "b".repeat(width);
        let words = vec![
            "--no-verify".into(),
            format!("--force-with-lease={destination}:{expected}"),
            "--".into(),
            "fork".into(),
            format!("refs/heads/topic:{destination}"),
        ];
        assert!(parse(&words)?.2);
        for tracking in [None, Some(expected.clone())] {
            assert_eq!(
                captured_lease(&words, destination, tracking, false)?,
                Some(expected.clone())
            );
        }
        assert!(captured_lease(&words, destination, Some(newer.clone()), false).is_err());
        assert!(captured_lease(&words, destination, Some(newer), true).is_err());
        let route = git::repository_tools::routes::PushRoute {
            branch: "topic".into(),
            commit: "c".repeat(width),
            remote: "fork".into(),
            destination: destination.into(),
            urls: vec!["https://forge.invalid/team/repo".into()],
            lease: captured_lease(&words, destination, None, false)?,
        };
        let mut plan = selected_continuation_recording::plan(&route);
        plan.original = vec![[vec!["push".into()], words].concat()];
        let original = plan.again();
        let approved = approved_ops(plan);
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0][0], APPROVED);
        let retained = registered(&approved[0][1])?;
        assert_eq!(retained.base.as_deref(), Some(expected.as_str()));
        assert_eq!(retained.again(), original);
        assert!(
            push_args(&retained)?.contains(&format!("--force-with-lease={destination}:{expected}"))
        );
        let reloaded_base = captured_lease(
            &retained.again()[0][1..],
            destination,
            Some("d".repeat(width)),
            false,
        );
        assert!(
            reloaded_base.is_err(),
            "reload must refuse refreshed tracking data"
        );
    }
    for lease in [
        "refs/heads/other:".to_owned() + &"a".repeat(40),
        "refs/heads/topic:short".into(),
        "refs/heads/topic:bad\0object".into(),
    ] {
        assert!(
            captured_lease(
                &[format!("--force-with-lease={lease}")],
                destination,
                None,
                false
            )
            .is_err()
        );
    }
    let conflicting = [
        format!("--force-with-lease={destination}:{}", "a".repeat(40)),
        format!("--force-with-lease={destination}:{}", "b".repeat(40)),
    ];
    assert!(captured_lease(&conflicting, destination, None, false).is_err());
    // Non-explicit generic force mode retains its prior fetched-tip semantics.
    let tracked = Some("e".repeat(40));
    assert_eq!(
        captured_lease(
            &["--force-with-lease".into()],
            destination,
            tracked.clone(),
            false
        )?,
        tracked
    );
    Ok(())
}

#[test]
fn recording_approved_fork_plan_retains_the_original_scope_lifetime() -> Result<(), String> {
    let hosting = crate::forge::Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "forge.invalid".into(),
        web: "https://forge.invalid".into(),
        path: "team/repo".into(),
    };
    let primary = git::Remote {
        name: "origin".into(),
        url: "https://forge.invalid/team/repo.git".into(),
    };
    let scope = crate::repo::forks::ExecutionScope::new(7, hosting, primary);
    let route = git::repository_tools::routes::PushRoute {
        branch: "topic".into(),
        commit: "c".repeat(40),
        remote: "fork".into(),
        destination: "refs/heads/topic".into(),
        urls: vec!["https://forge.invalid/team/fork".into()],
        lease: Some("a".repeat(40)),
    };
    let mut plan = selected_continuation_recording::plan(&route);
    plan.retain_fork_scope(scope.clone());
    let approved = approved_ops(plan);
    let marker = approved.last().ok_or("missing APPROVED marker")?;
    assert_eq!(marker[0], APPROVED);
    let retained = registered(&marker[1])?;
    let reload_scope = retained
        .fork_scope()
        .cloned()
        .ok_or("missing captured scope")?;
    assert_eq!(reload_scope.id, scope.id);
    assert!(reload_scope.check().is_ok());
    scope.invalidate();
    // These checks are entirely in-memory and fail before any remote discovery.
    assert!(reload_scope.check().is_err());
    assert!(
        retained
            .fork_scope()
            .ok_or("lost original scope")?
            .check()
            .is_err()
    );
    Ok(())
}

/// Actual generic DCO argv forms, supplied without repository or runtime discovery.
#[test]
fn recording_dco_shorthand_and_expected_absent_keep_their_distinct_semantics() -> Result<(), String>
{
    let destination = "refs/heads/topic";
    let oid = "a".repeat(40);
    let shorthand = vec![
        format!("--force-with-lease={destination}"),
        "--".into(),
        "fork".into(),
        format!("HEAD:{destination}"),
    ];
    assert_eq!(
        captured_lease(&shorthand, destination, Some(oid.clone()), false)?,
        Some(oid.clone())
    );
    assert_eq!(captured_lease(&shorthand, destination, None, false)?, None);
    assert!(captured_lease(&shorthand, "refs/heads/other", None, false).is_err());
    assert!(captured_lease(&shorthand, destination, Some(oid.clone()), true).is_err());
    let absent = vec![
        format!("--force-with-lease={destination}:"),
        "--".into(),
        "fork".into(),
        format!("HEAD:{destination}"),
    ];
    assert_eq!(captured_lease(&absent, destination, None, false)?, None);
    assert!(captured_lease(&absent, destination, Some(oid.clone()), false).is_err());
    assert!(captured_lease(&absent, destination, None, true).is_err());
    let fixed = format!("--force-with-lease={destination}:{oid}");
    let empty = format!("--force-with-lease={destination}:");
    for contradictory in [
        vec![empty.clone(), fixed.clone()],
        vec![fixed, empty.clone()],
    ] {
        assert!(captured_lease(&contradictory, destination, None, false).is_err());
    }
    assert_eq!(
        captured_lease(&[empty.clone(), empty], destination, None, false)?,
        None
    );
    let route = git::repository_tools::routes::PushRoute {
        branch: "topic".into(),
        commit: "c".repeat(40),
        remote: "fork".into(),
        destination: destination.into(),
        urls: vec!["https://forge.invalid/team/fork".into()],
        lease: None,
    };
    let mut plan = selected_continuation_recording::plan(&route);
    plan.force = true;
    plan.original = vec![[vec!["push".into()], absent].concat()];
    let args = push_args(&plan)?;
    assert!(args.contains(&format!("--force-with-lease={destination}:")));
    let approved = approved_ops(plan.clone());
    let retained = registered(&approved[0][1])?;
    assert_eq!(retained.base, None);
    assert_eq!(push_args(&retained)?, args);
    plan.base = Some(oid.clone());
    assert!(
        push_args(&plan).is_err(),
        "expected-absent must refuse any tracked object"
    );
    plan.original = vec![[vec!["push".into()], shorthand].concat()];
    assert!(push_args(&plan)?.contains(&format!("--force-with-lease={destination}:{oid}")));
    plan.base = None;
    assert!(
        push_args(&plan).is_err(),
        "shorthand without a fetched tip still cannot force-send"
    );
    Ok(())
}
