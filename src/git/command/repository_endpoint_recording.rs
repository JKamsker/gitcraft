use super::*;
#[cfg(windows)]
#[test]
fn reviewed_endpoint_and_account_survive_normalization_without_reinterpretation()
-> Result<(), String> {
    let repo = Path::new("C:/recorded/repo");
    let url = "https://host-a.invalid/team/repo";
    let creds = Credentials::for_url(url, "alice".into(), "DUMMY_A_TOKEN".into())?;
    let result = repository_endpoint_extra(repo, url, "origin", Some(&creds), &["push"]);
    assert!(result.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
    // An account scoped to another host needs no transient credential transport.
    let (extra, _) = repository_endpoint_extra(
        repo,
        "https://host-b.invalid/team/repo",
        "origin",
        Some(&creds),
        &["push"],
    )?;
    assert!(extra.owned_pre.is_empty());
    assert!(!extra.env.iter().any(|(k, _)| k.starts_with("GITCRAFT_")));
    Ok(())
}
#[cfg(unix)]
#[test]
fn reviewed_endpoint_and_account_survive_normalization_without_reinterpretation()
-> Result<(), String> {
    let repo = Path::new("/recorded/repo");
    let url = "https://host-a.invalid/team/repo";
    let credentials = Credentials::for_url(url, "alice".into(), "DUMMY_A_TOKEN".into())?;
    for address in [url, "https://host-b.invalid/team/repo", "origin"] {
        let (extra, alias) =
            repository_endpoint_extra(repo, address, "origin", Some(&credentials), &["push"])?;
        let args = ["push", "--porcelain", "--", &alias, "a:refs/heads/main"];
        let mut selected = Command::new("/recorded/chosen/git");
        clear_repository_environment(&mut selected);
        let cmd = apply_command_extras(selected, None, &extra, &args);
        let env: std::collections::BTreeMap<_, _> = cmd.get_envs().collect();
        let get = |k: &str| env.get(std::ffi::OsStr::new(k)).copied().flatten();
        assert_eq!(get("GIT_CONFIG_COUNT"), Some(std::ffi::OsStr::new("9")));
        assert_eq!(
            get("GIT_CONFIG_KEY_0"),
            Some(std::ffi::OsStr::new(&format!("url.{address}.insteadOf")))
        );
        assert_eq!(get("GIT_CONFIG_VALUE_0"), get("GIT_CONFIG_VALUE_2"));
        assert_eq!(get("GIT_CONFIG_VALUE_2"), get("GIT_CONFIG_VALUE_4"));
        assert_eq!(get("GIT_CONFIG_VALUE_4"), get("GIT_CONFIG_VALUE_8"));
        for name in REPOSITORY_ENV_REMOVED {
            if *name != "GIT_CONFIG_COUNT" && *name != "GIT_CONFIG_GLOBAL" {
                assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None), "{name}");
            }
        }
        assert_eq!(get("GITCRAFT_GIT_PASSWORD").is_some(), address == url);
        if address == url {
            assert_eq!(
                get("GITCRAFT_GIT_ORIGINAL"),
                Some(std::ffi::OsStr::new("/recorded/chosen/git"))
            );
        }
        assert!(
            !cmd.get_args()
                .any(|a| a.to_string_lossy().contains("DUMMY_A_TOKEN"))
        );
        let target = remote::Target {
            host: remote::Host::Ssh("uncontacted.invalid".into()),
            path: "/repo".into(),
        };
        let request = remote_request(&target, &extra, &args);
        assert_eq!(
            request.script.starts_with(CREDENTIAL_REMOTE_PIN),
            address == url
        );
        assert!(request.unset.iter().any(|k| k == "GIT_CONFIG_COUNT"));
        assert!(
            request
                .env
                .contains(&("GIT_CONFIG_COUNT".into(), "9".into()))
        );
    }
    let legacy = Credentials {
        scope: None,
        ..credentials.clone()
    };
    assert!(repository_endpoint_extra(repo, url, "origin", Some(&legacy), &["push"]).is_err());
    let edited = Credentials {
        username: "bob".into(),
        ..credentials
    };
    assert!(repository_endpoint_extra(repo, url, "origin", Some(&edited), &["push"]).is_err());
    Ok(())
}
