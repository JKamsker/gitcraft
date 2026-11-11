#[test]
fn reviewed_auth_receipts_keep_the_actual_failing_endpoint_out_of_logs() {
    let a = "https://alice:DO_NOT_LOG@a.invalid/team/repo.git?private=DUMMY";
    let b = "https://bob@b.invalid/another/repo.git";
    let ea = remember_auth_endpoint(a);
    let eb = remember_auth_endpoint(b);
    assert!(git::is_auth_error(&ea));
    assert!(!ea.contains("DO_NOT_LOG") && !ea.contains("a.invalid"));
    assert_eq!(take_auth_endpoint(&eb).as_deref(), Some(b));
    assert_eq!(take_auth_endpoint(&ea).as_deref(), Some(a));
    assert!(take_auth_endpoint(&ea).is_none());
    assert!(is_reviewed_auth_error(&ea)); // expiration never permits a fetch-URL fallback
}

#[cfg(unix)]
#[test]
fn recording_push_only_placeholder_rewrite_cannot_change_the_reviewed_receiver()
-> Result<(), String> {
    let f = Fixture::new()?;
    let intended = f
        .root
        .join("uncontacted-A.git")
        .to_string_lossy()
        .into_owned();
    let other = f
        .root
        .join("uncontacted-B.git")
        .to_string_lossy()
        .into_owned();
    f.git(&[
        "config",
        &format!("url.{other}.pushInsteadOf"),
        "gitcraft-preflight-address://",
    ])?;
    let marker = f.root.join("receiver-argv");
    let shim = f.root.join("record-receiver");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" > {}\nexit 1\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for force in [false, true] {
        let head = f.git(&["rev-parse", "HEAD"])?;
        let alias = "gitcraft-fixture-recorded";
        let mut args = vec!["push".into(), "--dry-run".into(), "--no-verify".into()];
        if force {
            args.push(format!("--force-with-lease=refs/heads/main:{head}"));
        }
        args.extend([alias.into(), format!("{head}:refs/heads/main")]);
        let result = git::push_preflight_with(
            &f.repo,
            &args,
            git::PushTarget {
                address: &intended,
                alias,
                original_remote: "origin",
                skip_hooks: true,
                lfs_config: &[],
                lfs_objects: &[],
                receive_pack: shim.to_str().ok_or("shim path")?,
            },
            None,
            None,
        );
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&marker)
                .map_err(|e| format!("{e}: {result:?}"))?
                .trim(),
            intended
        );
        assert!(!std::path::Path::new(&intended).exists());
        assert!(!std::path::Path::new(&other).exists());
    }
    Ok(())
}

#[test]
fn push_pins_rewritten_url_without_putting_it_in_argv_and_never_follows_tags() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let other = f.root.join("wrong.git");
    git::run(None, &["init", "--bare", "-q", &other.to_string_lossy()])?;
    f.git(&[
        "config",
        &format!("url.{}.insteadOf", other.display()),
        "gitcraft-preflight-address://",
    ])?;
    let plan = f.plan(f.profile())?;
    let head = plan.head.clone();
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, head);
    assert!(git::resolve(&other, "refs/heads/main").is_err());
    let args = push_args(&plan)?;
    assert!(args.contains(&"--no-follow-tags".into()));
    assert!(args.contains(&"--recurse-submodules=no".into()));
    assert!(!args.iter().any(|a| a.contains("withheld")));
    Ok(())
}

#[test]
fn remote_credentials_and_query_values_are_not_in_checklist_targets() -> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    f.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        "https://credential_user:DO_NOT_DISPLAY@github.com/personal/repo.git?token=DO_NOT_DISPLAY",
    ])?;
    let mut profile = f.profile();
    profile
        .author_hosts
        .push(crate::settings::profiles::HostAuthor {
            host: "github.com/personal".into(),
            name: "Personal".into(),
            email: "personal@example.org".into(),
        });
    let plan = f.plan(profile)?;
    assert_eq!(plan.expected_email, "personal@example.org");
    assert!(plan.findings.iter().any(|i| i.kind == Kind::Identity));
    assert_eq!(plan.targets, ["github.com/personal/repo"]);
    assert!(
        plan.findings
            .iter()
            .all(|i| !i.detail.contains("DO_NOT_DISPLAY"))
    );
    Ok(())
}

#[test]
fn explicit_annotated_tag_push_works_with_detached_head() -> Result<(), String> {
    let f = Fixture::new()?;
    f.git(&["tag", "-a", "-m", "Release", "release"])?;
    let tag = f.git(&["rev-parse", "refs/tags/release"])?;
    f.git(&["checkout", "-q", "--detach"])?;
    let checkout = f.git(&["rev-parse", "HEAD"])?;
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "origin".into(),
            "refs/tags/release".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    assert!(plan.checkout.is_empty());
    assert_eq!(plan.checkout_oid, checkout);
    assert_eq!(plan.push_oid, tag);
    execute(&f.repo, &plan, None)?;
    assert_eq!(
        git::run(Some(&f.receiver), &["rev-parse", "refs/tags/release"])?.trim(),
        tag
    );
    assert_eq!(f.git(&["rev-parse", "HEAD"])?, checkout);
    Ok(())
}

#[test]
fn routed_submodule_checks_survive_preflight_without_enabling_child_publication()
-> Result<(), String> {
    let f = Fixture::new()?;
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "--recurse-submodules=check".into(),
            "origin".into(),
            "HEAD:refs/heads/main".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    let args = push_args(&plan)?;
    assert_eq!(
        args.iter()
            .filter(|s| s.starts_with("--recurse-submodules="))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["--recurse-submodules=check"]
    );
    assert!(parse(&["--recurse-submodules=on-demand".into()]).is_err());
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, plan.head);
    Ok(())
}
