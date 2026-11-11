use super::*;
#[test]
fn consented_sparse_uses_captured_account_and_restores_passive_policy() -> Result<(), String> {
    let profile = crate::connections::Context {
        git_config: Some("/recorded/profile-a".into()),
        ..Default::default()
    };
    profile.run(|| {
        let account = Credentials::for_url(
            "https://example.invalid/team/repo",
            "account-A".into(),
            "SYNTHETIC_A_ONLY".into(),
        )?;
        // Same value captured by the Apply worker, even if the tab later signs in as B.
        let captured = account.clone();
        for repo in [
            Path::new("/recorded/repo"),
            Path::new("ssh://recorded/repo"),
        ] {
            for consent in [true, false] {
                super::super::clone_options::sparse_mutation(
                    repo,
                    Some(&["src".into()]),
                    consent,
                    Some(&captured),
                    |dir, args, credentials| {
                        assert_eq!(dir, Some(repo));
                        assert_eq!(
                            credentials.map(|c| c.username.as_str()),
                            consent.then_some("account-A")
                        );
                        let extra = extra_for(dir, args, credentials, false)?;
                        let mut cmd = Command::new("/recorded/git");
                        cmd.env("GITCRAFT_GIT_USERNAME", "ambient-B");
                        object_read_policy(&mut cmd, args);
                        with_profile(&mut cmd, args);
                        // None avoids the filesystem-only practice-repository classifier;
                        // command discovery/launch is never entered by this recorder.
                        let cmd = apply_command_extras(cmd, None, &extra, args);
                        let env: std::collections::BTreeMap<_, _> = cmd
                            .get_envs()
                            .map(|(k, v)| {
                                (
                                    k.to_string_lossy().into_owned(),
                                    v.map(|v| v.to_string_lossy().into_owned()),
                                )
                            })
                            .collect();
                        assert_eq!(
                            env.get("GIT_CONFIG_GLOBAL"),
                            Some(&Some("/recorded/profile-a".into()))
                        );
                        assert_eq!(
                            env.get("GIT_NO_LAZY_FETCH"),
                            Some(&Some(if consent { "0" } else { "1" }.into()))
                        );
                        if consent {
                            assert_eq!(
                                env.get("GITCRAFT_GIT_USERNAME"),
                                Some(&Some("account-A".into()))
                            );
                            assert_eq!(
                                env.get("GITCRAFT_GIT_PASSWORD"),
                                Some(&Some("SYNTHETIC_A_ONLY".into()))
                            );
                        } else {
                            assert!(
                                !extra
                                    .env
                                    .iter()
                                    .any(|(key, _)| *key == "GITCRAFT_GIT_PASSWORD")
                            );
                        }
                        let argv: Vec<_> = cmd
                            .get_args()
                            .map(|a| a.to_string_lossy().into_owned())
                            .collect();
                        assert_eq!(
                            argv.iter().any(|a| a == "gitcraft-credential-transport"),
                            consent
                        );
                        assert_eq!(argv.iter().any(|a| a == "--no-lazy-fetch"), !consent);
                        assert!(!argv.join(" ").contains("SYNTHETIC_A_ONLY"));
                        let req = remote_request(
                            &Target {
                                host: remote::Host::Ssh("recorded".into()),
                                path: "/repo".into(),
                            },
                            &extra,
                            args,
                        );
                        assert_eq!(
                            req.env
                                .iter()
                                .any(|(k, v)| k == "GITCRAFT_GIT_USERNAME" && v == "account-A"),
                            consent
                        );
                        assert_eq!(
                            req.script.contains("gitcraft-credential-transport"),
                            consent
                        );
                        assert_eq!(req.script.contains("--no-lazy-fetch"), !consent);
                        assert!(!req.script.contains("SYNTHETIC_A_ONLY"));
                        Ok(String::new())
                    },
                )?;
            }
        }
        for args in [["rev-parse", "HEAD"], ["show", "HEAD:file"]] {
            let extra = extra_for(None, &args, None, false)?;
            assert!(extra.env.is_empty());
            let mut cmd = Command::new("/recorded/git");
            object_read_policy(&mut cmd, &args);
            let cmd = apply_command_extras(cmd, None, &extra, &args);
            assert_eq!(
                cmd.get_args().next(),
                Some(std::ffi::OsStr::new("--no-lazy-fetch"))
            );
            assert!(!cmd.get_envs().any(|(k, _)| k == "GITCRAFT_GIT_PASSWORD"));
        }
        Ok(())
    })
}
