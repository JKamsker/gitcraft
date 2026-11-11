//! Captured repository and web-authority rejection cases.
use super::*;

pub(super) fn provider_boundary_ssh_selects_cross_host_secondary_and_checks_configured_login()
-> Result<(), Box<dyn std::error::Error>> {
    for (forge, host, origin) in [
        (
            Forge::GitHub,
            "github.review9-trusted.test",
            "https://github.com/upstream/project",
        ),
        (
            Forge::GitLab,
            "gitlab.review9-trusted.test",
            "https://gitlab.com/upstream/project",
        ),
    ] {
        let h = hosting(forge, host);
        let tool = if forge == Forge::GitHub { "gh" } else { "glab" };
        let calls = recording::run(vec![ok("signed in"), ok("[]")], || {
            recording::set_remotes(vec![
                Remote {
                    name: "origin".into(),
                    url: origin.into(),
                },
                Remote {
                    name: "fork".into(),
                    url: format!("git@{host}:fork/project.git"),
                },
            ]);
            assert!(
                forge
                    .provider()
                    .list(Path::new("ssh://review9-host/tmp/repo"), &h)?
                    .is_empty()
            );

            Ok(())
        })?;
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool, tool);
        assert_eq!(calls[0].args, ["auth", "status", "--hostname", host]);
        for secret in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GITLAB_TOKEN",
            "GITLAB_ACCESS_TOKEN",
            "OAUTH_TOKEN",
            "CI_JOB_TOKEN",
        ] {
            assert!(calls[0].env.iter().any(|(k, v)| k == secret && v.is_none()));
        }
        let expected = if forge == Forge::GitHub {
            format!("{host}/fork/project")
        } else {
            format!("https://{host}/fork/project")
        };
        assert_eq!(flag(&calls[1], "--repo"), Some(expected.as_str()));
    }

    Ok(())
}

pub(super) fn provider_boundary_ssh_rejects_untrusted_invalid_and_removed_targets_without_forge_call()
-> Result<(), Box<dyn std::error::Error>> {
    for forge in [Forge::GitHub, Forge::GitLab] {
        let host = if forge == Forge::GitHub {
            "github.review9-untrusted.test"
        } else {
            "gitlab.review9-untrusted.test"
        };
        let h = hosting(forge, host);
        let calls = recording::run(vec![Err("not signed in".into())], || {
            recording::set_remotes(vec![Remote {
                name: "fork".into(),
                url: format!("https://{host}/fork/project"),
            }]);
            let e = forge
                .provider()
                .retarget(Path::new("ssh://review9-host/tmp/repo"), &h, 7, "main")
                .err()
                .ok_or("expected error")?;
            assert!(e.contains("isn't signed in"), "{e}");

            Ok(())
        })?;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args[0], "auth");
        for h in [
            Hosting {
                host: "--evil".into(),
                ..h.clone()
            },
            Hosting {
                path: "../bad/project".into(),
                ..h.clone()
            },
            h,
        ] {
            let calls = recording::run(vec![], || {
                recording::set_remotes(vec![Remote {
                    name: "origin".into(),
                    url: "https://github.com/upstream/project".into(),
                }]);
                assert!(
                    forge
                        .provider()
                        .retarget(Path::new("ssh://review9-host/tmp/repo"), &h, 7, "main")
                        .is_err()
                );

                Ok(())
            })?;
            assert!(
                calls.is_empty(),
                "invalid/removed target must not fall back to origin"
            );
        }
    }

    Ok(())
}

pub(super) fn provider_boundary_ci_reads_and_mutations_use_the_selected_project()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir();
    let h = hosting(Forge::GitHub, "github.enterprise.test");
    let project = crate::ci::Project {
        forge: h.forge,
        host: h.host.clone(),
        path: h.path.clone(),
    };
    capture(&h, vec![ok("[]")], || {
        crate::ci::runs(&dir, &project, &"a".repeat(40))?;

        Ok(())
    })?;
    capture(&h, vec![ok("")], || {
        crate::ci::cancel(&dir, &project, 7)?;

        Ok(())
    })?;
    capture(&h, vec![ok("")], || {
        crate::ci::rerun_failed(&dir, &project, 7)?;

        Ok(())
    })?;
    let h = hosting(Forge::GitLab, "gitlab.enterprise.test");
    let project = crate::ci::Project {
        forge: h.forge,
        host: h.host.clone(),
        path: h.path.clone(),
    };
    capture(&h, vec![ok("[]")], || {
        crate::ci::runs(&dir, &project, &"a".repeat(40))?;

        Ok(())
    })?;
    capture(&h, vec![ok("")], || {
        crate::ci::cancel(&dir, &project, 7)?;

        Ok(())
    })?;
    capture(&h, vec![ok("")], || {
        crate::ci::rerun_failed(&dir, &project, 7)?;

        Ok(())
    })?;

    Ok(())
}

pub(super) fn provider_boundary_gitlab_port_is_kept_for_api_and_release_targets()
-> Result<(), Box<dyn std::error::Error>> {
    let h = Hosting {
        web: "https://gitlab.port.test:8443".into(),
        ..hosting(Forge::GitLab, "gitlab.port.test")
    };
    let dir = std::env::temp_dir();
    let calls = recording::run(vec![ok("[]"), ok("")], || {
        gitlab::GitLab.forks(&dir, &h)?;
        gitlab::GitLab.upload_assets(&dir, &h, "v1", &["asset.zip".into()])?;

        Ok(())
    })?;
    assert_eq!(flag(&calls[0], "--hostname"), Some("gitlab.port.test:8443"));
    assert_eq!(flag(&calls[0], "--repo"), None);
    assert_eq!(
        flag(&calls[1], "--repo"),
        Some("https://gitlab.port.test:8443/fork/project")
    );
    assert!(
        calls[0]
            .env
            .iter()
            .any(|(k, v)| k == "GITLAB_API_HOST" && v.as_deref() == Some("gitlab.port.test:8443"))
    );

    Ok(())
}

pub(super) fn provider_boundary_same_numbers_in_fork_and_upstream_do_not_change_git_transport()
-> Result<(), Box<dyn std::error::Error>> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-provider-routing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    for args in [
        vec!["init", "-q"],
        vec![
            "remote",
            "add",
            "origin",
            "https://github.com/upstream/project",
        ],
        vec![
            "remote",
            "add",
            "fork",
            "https://github.enterprise.test/fork/project",
        ],
        vec!["config", "branch.main.remote", "origin"],
        vec!["config", "branch.main.merge", "refs/heads/main"],
        vec!["config", "remote.pushDefault", "origin"],
    ] {
        crate::git::run(Some(&dir), &args)?;
    }
    let config = std::fs::read(dir.join(".git/config"))?;
    for h in [
        hosting(Forge::GitHub, "github.enterprise.test"),
        Hosting {
            path: "upstream/project".into(),
            ..hosting(Forge::GitHub, "github.com")
        },
    ] {
        let answer = serde_json::json!({"number":7,"title":h.path,"url":format!("{}/{}/issues/7",h.web,h.path)}).to_string();
        capture(&h, vec![ok(&answer)], || {
            assert_eq!(github::GitHub.issue(&dir, &h, 7)?.title, h.path);

            Ok(())
        })?;
    }
    assert_eq!(
        std::fs::read(dir.join(".git/config"))?,
        config,
        "forge routing must never rewrite remotes, pushDefault or upstreams"
    );

    Ok(())
}

pub(super) fn provider_boundary_release_blocks_incompatible_pushurl_before_any_cli_call()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-release-publication-target-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    crate::git::run(Some(&dir), &["init", "-q"])?;
    crate::git::run(
        Some(&dir),
        &["remote", "add", "fork", "https://github.com/fork/project"],
    )?;
    crate::git::run(
        Some(&dir),
        &[
            "remote",
            "set-url",
            "--push",
            "fork",
            "https://github.com/upstream/project",
        ],
    )?;
    let h = hosting(Forge::GitHub, "github.com");
    let plan = crate::release::Plan {
        commit: "a".repeat(40),
        tag: "v1".into(),
        notes: "notes".into(),
        bump: None,
        sign: false,
        push: Some("fork".into()),
        publish: true,
        prerelease: false,
        assets: vec!["asset.zip".into()],
    };
    for step in [crate::release::Step::Publish, crate::release::Step::Upload] {
        let calls = recording::run(vec![], || {
            let mut done = crate::release::Done::default();
            let result = crate::release::run_step(&dir, &plan, step, &mut done, None, Some(&h));
            assert!(
                result.is_err_and(|e| e.contains("does not match the selected forge repository"))
            );
            Ok(())
        })?;
        assert!(calls.is_empty());
    }
    crate::git::run(
        Some(&dir),
        &[
            "remote",
            "set-url",
            "--push",
            "fork",
            "https://github.com/fork/project",
        ],
    )?;
    capture(
        &h,
        vec![ok("https://github.com/fork/project/releases/tag/v1")],
        || {
            let mut done = crate::release::Done::default();
            crate::release::run_step(
                &dir,
                &plan,
                crate::release::Step::Publish,
                &mut done,
                None,
                Some(&h),
            )?;
            assert!(done.url.is_some());
            Ok(())
        },
    )?;
    Ok(())
}

pub(super) fn provider_boundary_creation_url_cannot_override_selected_repository_for_reviewers()
-> Result<(), Box<dyn std::error::Error>> {
    for forge in [Forge::GitHub, Forge::GitLab] {
        let host = if forge == Forge::GitHub {
            "github.com"
        } else {
            "gitlab.com"
        };
        let marker = if forge == Forge::GitHub {
            "pull"
        } else {
            "-/merge_requests"
        };
        let h = hosting(forge, host);
        let dir = std::env::temp_dir();
        let unexpected = format!("https://{host}/upstream/project/{marker}/7");
        let calls = capture(&h, vec![ok(&unexpected)], || {
            assert!(
                forge
                    .provider()
                    .create(&dir, &h, &new_pr(&["reviewer".into()]))
                    .is_err_and(|e| e.contains("reviewers were not requested"))
            );
            Ok(())
        })?;
        assert_eq!(
            calls.len(),
            1,
            "no follow-up may use the unexpected URL's number"
        );
    }
    Ok(())
}

pub(super) fn review13_creation_and_review_targets_validate_the_web_instance() -> Result<(), String>
{
    for forge in Forge::ALL {
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            path: "Team/Repo".into(),
            web: "https://forge.example:4000".into(),
        };
        let marker = if forge == Forge::GitLab {
            "-/merge_requests"
        } else {
            "pull"
        };
        for (base, path, expected) in [
            ("https://forge.example:3000", "Team/Repo", false),
            ("http://forge.example:4000", "Team/Repo", false),
            (
                "https://forge.example:4000",
                "team/repo",
                !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket),
            ),
            ("HTTPS://FORGE.example:4000", "Team/Repo", true),
        ] {
            let url = format!("{base}/{path}/{marker}/7");
            assert_eq!(
                created_pr_number(&h, &url).is_ok(),
                expected,
                "{forge:?} {url}"
            );
            if forge == Forge::GitHub {
                let details = crate::pull_request::PrDetails {
                    number: 7,
                    url,
                    ..Default::default()
                };
                assert_eq!(
                    github::GitHub
                        .target(Path::new("/nonexistent/review13"), &h, &details)
                        .is_some(),
                    expected
                );
            }
        }
        if matches!(forge, Forge::GitHub | Forge::GitLab) {
            let unexpected = format!("https://forge.example:3000/Team/Repo/{marker}/7");
            let calls = capture(&h, vec![ok(&unexpected)], || {
                assert!(
                    forge
                        .provider()
                        .create(&std::env::temp_dir(), &h, &new_pr(&["reviewer".into()]))
                        .is_err_and(|e| e.contains("reviewers were not requested"))
                );
                Ok(())
            })?;
            assert_eq!(
                calls.len(),
                1,
                "mismatched create URL must stop before follow-up mutation"
            );
        }
    }
    Ok(())
}
