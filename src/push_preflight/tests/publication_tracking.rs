#[test]
fn publication_tracking_matches_native_push_upstream_rebase_semantics() -> Result<(), String> {
    let f = Fixture::new()?;
    for mode in ["never", "local", "remote", "always"] {
        f.git(&["config", "branch.autoSetupRebase", mode])?;
        for (index, prior) in [None, Some("false"), Some("true")].into_iter().enumerate() {
            let native = format!("native-{mode}-{index}");
            let published = format!("published-{mode}-{index}");
            for branch in [&native, &published] {
                f.git(&["update-ref", &format!("refs/heads/{branch}"), "HEAD"])?;
                if let Some(prior) = prior {
                    f.git(&["config", &format!("branch.{branch}.rebase"), prior])?;
                }
            }
            f.git(&["push", "--set-upstream", "origin", &format!("refs/heads/{native}:refs/heads/{native}")])?;
            let prepared = git::repository_tools::routes::prepare_publication(&f.repo, &published, "origin", &published)?;
            let mut plan = scan(&f.repo, prepared.route.publication_operations(), f.profile(), &|| true)?;
            plan.retain_selection(prepared)?;
            execute(&f.repo, &plan, None)?;
            let rebase = |branch: &str| {
                git::run_ok_codes(Some(&f.repo), &["config", "--get-all", &format!("branch.{branch}.rebase")], &[0, 1])
            };
            assert_eq!(rebase(&published)?, rebase(&native)?, "{mode}, prior={prior:?}");
            assert_eq!(f.git(&["config", &format!("branch.{published}.remote")])?, "origin");
            assert_eq!(f.git(&["config", &format!("branch.{published}.merge")])?, format!("refs/heads/{published}"));
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn publication_hook_rejection_preserves_tracking_and_rebase() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    let hooks = f.root.join("reject-hooks");
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    let hook = hooks.join("pre-push");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    f.git(&["config", "core.hooksPath", &hooks.to_string_lossy()])?;
    f.git(&["config", "branch.autoSetupRebase", "always"])?;
    f.git(&["config", "branch.main.rebase", "false"])?;
    let before = f.git(&["config", "--null", "--list"])?;
    let prepared = git::repository_tools::routes::prepare_publication(&f.repo, "main", "origin", "hook-rejected")?;
    let mut plan = scan(&f.repo, prepared.route.publication_operations(), f.profile(), &|| true)?;
    plan.retain_selection(prepared)?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.git(&["config", "--null", "--list"])?, before);
    assert!(git::run(Some(&f.receiver), &["for-each-ref", "refs/heads/hook-rejected"])?.is_empty());
    Ok(())
}
