use super::*;
/// Production builder only: selected programs are fixed strings and never run.
#[cfg(windows)]
#[test]
fn recording_reviewed_push_policy_preserves_counted_alias_and_exact_subcommand()
-> Result<(), String> {
    let url = "https://fixture.invalid/team/repo.git";
    let creds = Credentials::for_url(url, "alice".into(), "DUMMY_ONLY".into())?;
    let result = reviewed_push_extra(
        Path::new("C:/recorded/repo"),
        &["push"],
        PushTarget {
            address: url,
            alias: "gitcraft-preflight-fixture",
            original_remote: "origin",
            skip_hooks: false,
            lfs_config: &[],
            lfs_objects: &[],
            receive_pack: "",
        },
        "C:/recorded/hooks".into(),
        Some(&creds),
    );
    assert!(result.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
    Ok(())
}
#[cfg(unix)]
#[test]
fn recording_reviewed_push_policy_preserves_counted_alias_and_exact_subcommand()
-> Result<(), String> {
    let address = "https://fixture.invalid/team/repo.git";
    let alias = "gitcraft-preflight-fixture";
    let creds = Credentials::for_url(address, "alice".into(), "DUMMY_ONLY".into())?;
    let source = "a".repeat(40);
    let refspec = format!("{source}:refs/heads/main");
    let words = [
        "push",
        "--porcelain",
        "--force-with-lease=refs/heads/main:",
        alias,
        &refspec,
    ];
    let target = |address| PushTarget {
        address,
        skip_hooks: false,
        alias,
        original_remote: "origin",
        lfs_config: &[],
        lfs_objects: &[],
        receive_pack: "",
    };
    let repo = Path::new("/nonexistent/recorded-reviewed-push");
    let extra = reviewed_push_extra(
        repo,
        &words,
        target(address),
        "/owned/pinned-hooks".into(),
        Some(&creds),
    )?;
    assert!(extra.normalize_repository);
    assert_eq!(
        extra.owned_pre.last().map(String::as_str),
        Some("gitcraft-credential-transport")
    );
    let mut command = Command::new("/recorded/git");
    command
        .arg("-C")
        .arg(repo)
        .env("GIT_DIR", "/foreign/repo")
        .env("GIT_CONFIG_COUNT", "99");
    let command = apply_command_extras(command, Some(repo), &extra, &words);
    let args = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(args.ends_with(&words.map(str::to_owned)));
    let push = args.len() - words.len();
    assert_eq!(args[push - 1], "gitcraft-credential-transport");
    assert!(!args.join(" ").contains("DUMMY_ONLY"));
    let env = command
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(env.get("GIT_DIR"), Some(&None));
    assert_eq!(env.get("GIT_CONFIG_COUNT"), Some(&Some("10".into())));
    assert_eq!(
        env.get("GITCRAFT_GIT_ORIGINAL"),
        Some(&Some("/recorded/git".into()))
    );
    assert!(!env.contains_key("GITCRAFT_CREDENTIAL_OWNER"));
    assert_eq!(
        env.get("GITCRAFT_GIT_PASSWORD"),
        Some(&Some("DUMMY_ONLY".into()))
    );
    let configs = (0..10)
        .map(|i| {
            let value = |prefix| {
                let key = format!("{prefix}_{i}");
                env.get(&key)
                    .and_then(Option::as_ref)
                    .cloned()
                    .ok_or_else(|| format!("missing {key}"))
            };
            Ok((value("GIT_CONFIG_KEY")?, value("GIT_CONFIG_VALUE")?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let placeholder = format!("gitcraft-preflight-address://{alias}");
    for remote in [alias, "origin"] {
        for field in ["url", "pushurl"] {
            assert_eq!(
                configs
                    .iter()
                    .filter(|(k, _)| k == &format!("remote.{remote}.{field}"))
                    .map(|(_, v)| v.as_str())
                    .collect::<Vec<_>>(),
                vec!["", placeholder.as_str()]
            );
        }
    }
    assert!(configs.contains(&(format!("url.{address}.insteadOf"), placeholder)));
    assert!(configs.contains(&("core.hooksPath".into(), "/owned/pinned-hooks".into())));
    for trace_hooks in [false, true] {
        let mut remote_extra = extra.clone();
        remote_extra.trace_hooks = trace_hooks;
        let remote = remote::Target {
            host: remote::Host::Ssh("uncontacted.invalid".into()),
            path: "/repo".into(),
        };
        let request = remote_request(&remote, &remote_extra, &words);
        assert!(request.script.starts_with(CREDENTIAL_REMOTE_PIN));
        assert!(request.script.contains("\"$GITCRAFT_GIT_ORIGINAL\""));
        assert!(
            request
                .script
                .contains("'gitcraft-credential-transport' 'push' '--porcelain'")
        );
        assert!(!request.script.contains("DUMMY_ONLY"));
        assert!(
            request
                .env
                .contains(&("GIT_CONFIG_COUNT".into(), "10".into()))
        );
        assert!(
            !request
                .env
                .iter()
                .any(|(k, _)| k == "GITCRAFT_CREDENTIAL_OWNER")
        );
        assert!(request.unset.iter().any(|s| s == "GIT_CONFIG_COUNT"));
    }
    let other = reviewed_push_extra(
        repo,
        &words,
        target("https://other.invalid/repo"),
        "/owned/hooks".into(),
        Some(&creds),
    )?;
    assert!(other.owned_pre.is_empty());
    assert!(!other.env.iter().any(|(k, _)| k.starts_with("GITCRAFT_")));
    let edited = Credentials {
        username: "bob".into(),
        ..creds.clone()
    };
    assert!(
        reviewed_push_extra(
            repo,
            &words,
            target(address),
            "/hooks".into(),
            Some(&edited)
        )
        .is_err()
    );
    let legacy = Credentials {
        scope: None,
        ..creds
    };
    assert!(
        reviewed_push_extra(
            repo,
            &words,
            target("https://other.invalid/repo"),
            "/hooks".into(),
            Some(&legacy)
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn explicit_repository_context_clears_redirects_and_retains_private_index_overrides() {
    let private = Extra {
        normalize_repository: true,
        unset: vec!["GIT_INDEX_FILE", "GIT_CONFIG_COUNT", "GIT_SHALLOW_FILE"],
        env: vec![
            ("GIT_INDEX_FILE", "private.index".into()),
            ("GIT_CONFIG_COUNT", "1".into()),
            ("GIT_CONFIG_KEY_0", "remote.private.url".into()),
            ("GIT_CONFIG_VALUE_0", "https://push.invalid/repo".into()),
        ],
        ..Default::default()
    };
    let mut command = Command::new("/recorded/git");
    command.env("GIT_DIR", "/foreign/repo");
    clear_repository_environment(&mut command);
    let command = apply_command_extras(command, Some(Path::new("chosen")), &private, &["status"]);
    let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
    for key in REPOSITORY_ENV_REMOVED {
        if !["GIT_INDEX_FILE", "GIT_CONFIG_COUNT"].contains(key) {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None), "{key}");
        }
    }
    assert_eq!(
        env.get(std::ffi::OsStr::new("GIT_INDEX_FILE")),
        Some(&Some(std::ffi::OsStr::new("private.index")))
    );
    for (key, value) in &private.env {
        assert_eq!(
            env.get(std::ffi::OsStr::new(key)),
            Some(&Some(std::ffi::OsStr::new(value)))
        );
    }
    let target = remote::Target {
        host: remote::Host::Ssh("fixture.invalid".into()),
        path: "/chosen".into(),
    };
    let request = remote_request(&target, &private, &["status"]);
    for key in REPOSITORY_ENV_REMOVED {
        assert!(request.unset.iter().any(|s| s == key));
    }
    assert!(
        request
            .env
            .contains(&("GIT_INDEX_FILE".into(), "private.index".into()))
    );
}

#[test]
fn safety_and_operation_reads_select_the_same_explicit_repository() -> Result<(), String> {
    let a = crate::push_preflight::tests::Fixture::new()?;
    let b = crate::push_preflight::tests::Fixture::new()?;
    let expected = a.git(&["rev-parse", "--absolute-git-dir"])?;
    // A separate owned command models the shell's inherited redirect without
    // changing the process-global environment used by concurrent tests.
    let inherited =
        std::process::Command::new(crate::shell_path::executable("git").ok_or("git missing")?)
            .env("GIT_DIR", b.repo.join(".git"))
            .arg("-C")
            .arg(&a.repo)
            .args(["rev-parse", "--absolute-git-dir"])
            .output()
            .map_err(|e| e.to_string())?;
    assert_ne!(String::from_utf8_lossy(&inherited.stdout).trim(), expected);
    let ordinary = local_command(
        Some(&a.repo),
        &Extra::default(),
        &["rev-parse", "--absolute-git-dir"],
    )
    .output()
    .map_err(|e| e.to_string())?;
    let safety = forecast_run(
        &a.repo,
        &["rev-parse", "--absolute-git-dir"],
        &[],
        &[0],
        &|| true,
    )?;
    assert!(ordinary.status.success());
    assert_eq!(String::from_utf8_lossy(&ordinary.stdout).trim(), expected);
    assert_eq!(safety.trim(), expected);
    Ok(())
}
#[test]
fn recording_refusal_only_relays_known_status_for_the_reviewed_ref() -> Result<(), String> {
    let source = "a".repeat(40);
    let destination = "refs/heads/main";
    let good = format!(
        "server secret should disappear\n!\t{source}:{destination}\t[rejected] (stale info)\n"
    );
    let safe =
        reviewed_refusal(good.as_bytes(), &source, destination).ok_or("missing good refusal")?;
    assert!(!safe.contains("secret"));
    assert!(safe.ends_with("[rejected] (stale info)"));
    let hook = format!("!\t{source}:{destination}\t[remote rejected] (password=hidden-secret)\n");
    let safe =
        reviewed_refusal(hook.as_bytes(), &source, destination).ok_or("missing hook refusal")?;
    assert!(!safe.contains("hidden-secret"));
    assert!(safe.contains("[remote rejected]"));
    assert!(reviewed_refusal(good.as_bytes(), &source, "refs/heads/other").is_none());
    Ok(())
}
