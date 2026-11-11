use super::*;
#[cfg(windows)]
#[test]
fn root_unset_and_profile_precede_both_pinned_transport_environments() -> Result<(), String> {
    let url = "https://fixture.invalid/team/repo";
    let repo = Path::new("C:/recorded/repo");
    let creds = Credentials::for_url(url, "alice".into(), "DUMMY_MERGE".into())?;
    let context = crate::connections::Context {
        git_config: Some("C:/recorded/initiating-profile".into()),
        ..Default::default()
    };
    context.run(|| {
        let internals = repository_endpoint_extra(repo, url, "origin", Some(&creds), &["push"]);
        assert!(internals.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
        let reviewed = reviewed_push_extra(
            repo,
            &["push"],
            PushTarget {
                address: url,
                alias: "reviewed-fixture",
                original_remote: "origin",
                skip_hooks: false,
                lfs_config: &[],
                lfs_objects: &[],
                receive_pack: "",
            },
            "C:/recorded/hooks".into(),
            Some(&creds),
        );
        assert!(reviewed.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
    });
    Ok(())
}
#[cfg(unix)]
#[test]
fn root_unset_and_profile_precede_both_pinned_transport_environments() -> Result<(), String> {
    let url = "https://fixture.invalid/team/repo";
    let repo = Path::new("/recorded/repo");
    let creds = Credentials::for_url(url, "alice".into(), "DUMMY_MERGE".into())?;
    let context = crate::connections::Context {
        git_config: Some("/recorded/initiating-profile".into()),
        ..Default::default()
    };
    context.run(|| -> Result<(), String> {
        let (internals, alias) =
            repository_endpoint_extra(repo, url, "origin", Some(&creds), &["push"])?;
        let reviewed = reviewed_push_extra(
            repo,
            &["push"],
            PushTarget {
                address: url,
                alias: "reviewed-fixture",
                original_remote: "origin",
                skip_hooks: false,
                lfs_config: &[],
                lfs_objects: &[],
                receive_pack: "",
            },
            "/recorded/hooks".into(),
            Some(&creds),
        )?;
        for (mut extra, count, endpoint_alias) in [
            (internals, "9", alias.as_str()),
            (reviewed, "10", "reviewed-fixture"),
        ] {
            extra.unset.extend(["GIT_INDEX_FILE", "GIT_CONFIG_COUNT"]);
            extra.env.extend([
                ("GIT_INDEX_FILE", "/recorded/private-index".into()),
                ("GIT_DIR", "/recorded/command-source".into()),
            ]);
            let mut command = Command::new("/recorded/selected/git");
            command
                .arg("-C")
                .arg(repo)
                .env("GIT_DIR", "/foreign")
                .env("GIT_CONFIG_COUNT", "99");
            clear_repository_environment(&mut command);
            let words = [
                "push",
                "--porcelain",
                "--",
                endpoint_alias,
                "a:refs/heads/dev",
            ];
            // All program/directory strings are inert. None avoids discovery
            // and the native practice-registry probe during this recording.
            let command = apply_command_extras(command, None, &extra, &words);
            let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
            let value = |key: &str| env.get(std::ffi::OsStr::new(key)).copied().flatten();
            assert_eq!(value("GIT_CONFIG_COUNT"), Some(std::ffi::OsStr::new(count)));
            assert_eq!(
                value("GIT_INDEX_FILE"),
                Some(std::ffi::OsStr::new("/recorded/private-index"))
            );
            assert_eq!(
                value("GIT_DIR"),
                Some(std::ffi::OsStr::new("/recorded/command-source"))
            );
            assert_eq!(
                value("GIT_CONFIG_GLOBAL"),
                Some(std::ffi::OsStr::new("/recorded/initiating-profile"))
            );
            assert_eq!(
                value("GITCRAFT_GIT_ORIGINAL"),
                Some(std::ffi::OsStr::new("/recorded/selected/git"))
            );
            assert_eq!(
                value("GITCRAFT_GIT_PASSWORD"),
                Some(std::ffi::OsStr::new("DUMMY_MERGE"))
            );
            assert_eq!(value("GIT_GRAFT_FILE"), None);
            let argv: Vec<_> = command.get_args().collect();
            assert!(argv.iter().any(|a| *a == "gitcraft-credential-transport"));
            assert!(
                !argv
                    .iter()
                    .any(|a| a.to_string_lossy().contains("DUMMY_MERGE"))
            );
        }
        Ok(())
    })
}
