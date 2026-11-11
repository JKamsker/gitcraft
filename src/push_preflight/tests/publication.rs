#[test]
fn approved_push_is_pinned_and_refuses_new_server_tip_or_preexisting_unfetched_target()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next.txt", "next\n", "Next")?;
    let plan = f.plan(f.profile())?;
    assert!(plan.targets[0].contains(f.receiver.to_string_lossy().as_ref()));
    let remote = f.advance_receiver()?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.receiver_head()?, remote);
    f.git(&["fetch", "-q", "origin"])?;
    f.git(&["update-ref", "-d", "refs/remotes/origin/main"])?;
    let unknown = f.plan(f.profile())?;
    assert!(unknown.base.is_none());
    assert!(execute(&f.repo, &unknown, None).is_err());
    assert_eq!(f.receiver_head()?, remote);
    Ok(())
}

#[test]
fn stale_branch_checkout_identity_and_remote_configuration_all_refuse_before_sending()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let plan = f.plan(f.profile())?;
    let remote = f.receiver_head()?;
    f.git(&["checkout", "-qb", "other"])?;
    assert!(execute(&f.repo, &plan, None).is_err());
    f.git(&["checkout", "-q", "main"])?;
    f.git(&["config", "user.email", "changed@example.org"])?;
    assert!(execute(&f.repo, &plan, None).is_err());
    f.git(&["config", "user.email", "fixture@work.example"])?;
    let other = f.root.join("other.git");
    git::run(None, &["init", "--bare", "-q", &other.to_string_lossy()])?;
    f.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        &other.to_string_lossy(),
    ])?;
    assert!(execute(&f.repo, &plan, None).is_err());
    f.git(&["config", "--unset", "remote.origin.pushurl"])?;
    f.commit("later", "later", "Later")?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.receiver_head()?, remote);
    Ok(())
}

#[cfg(unix)]
#[test]
fn frozen_transport_preserves_the_original_remote_name_and_hook_configuration() -> Result<(), String>
{
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let hooks = f.root.join("custom-hooks");
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    let output = f.root.join("hook-arguments");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$1\" \"$2\" > {}\ngit config --get core.hooksPath >> {}\ncat >/dev/null\n",
        crate::remote::sh_quote(&output.to_string_lossy()),
        crate::remote::sh_quote(&output.to_string_lossy())
    );
    let hook = hooks.join("pre-push");
    std::fs::write(&hook, script).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    f.git(&["config", "core.hooksPath", &hooks.to_string_lossy()])?;
    let plan = f.plan(f.profile())?;
    execute(&f.repo, &plan, None)?;
    let arguments = std::fs::read_to_string(output).map_err(|e| e.to_string())?;
    let lines: Vec<_> = arguments.lines().collect();
    assert_eq!(lines.first(), Some(&"origin"));
    assert_eq!(
        lines.get(1).copied(),
        Some(f.receiver.to_string_lossy().as_ref())
    );
    assert_eq!(
        lines.get(2).copied(),
        Some(hooks.to_string_lossy().as_ref())
    );
    Ok(())
}

/// Supplied metadata only: no Fixture, Git, helper, filesystem or transport.
#[test]
fn merge_pure_reviewed_send_receipt_refuses_changed_exact_push() -> Result<(), String> {
    let oid = "a".repeat(40);
    let plan = Plan {
        branch: "refs/heads/topic".into(),
        remote: "team".into(),
        destination: "refs/heads/topic".into(),
        head: oid.clone(),
        base: None,
        commits: vec![],
        findings: vec![],
        targets: vec![],
        expected_name: String::new(),
        expected_email: String::new(),
        expected_accounts: BTreeMap::new(),
        profile: UserProfile::default(),
        force: false,
        deleting: false,
        tracking: false,
        lfs_objects: BTreeSet::new(),
        skip_hooks: false,
        push_oid: oid,
        original: vec![],
        urls: vec!["https://push.invalid/team/project".into()],
        checkout: String::new(),
        checkout_oid: String::new(),
        config: String::new(),
        config_before_push: String::new(),
        local_refs: BTreeMap::new(),
        extras: vec![],
        tracking_ref: None,
        lfs_config: vec![],
        receive_pack: "git-receive-pack".into(),
        selection: None,
        fork_scope: None,
    };
    let receipt = ReviewedSend::from_echo(push_args(&plan)?, None);
    receipt.check_plan(&plan)?;
    for field in 0..6 {
        let mut changed = plan.clone();
        match field {
            0 => changed.push_oid = "b".repeat(40),
            1 => changed.destination = "refs/heads/other".into(),
            2 => changed.remote = "different-receiver".into(),
            3 => changed.base = Some("c".repeat(40)),
            4 => changed.force = true, // no inspected lease: refuse even before dispatch
            _ => changed.extras.push("--recurse-submodules=check".into()),
        }
        assert!(
            receipt.check_plan(&changed).is_err(),
            "changed field {field}"
        );
    }
    Ok(())
}
