//! Process-boundary regressions. Answers are scripted after Command preparation; gh/glab
//! never execute, so the mutation cases cannot write to a live forge.
use super::cli::recording::{self, Call};
use super::*;
use crate::pr_post::{LineComment, MergeMethod, Verdict};
use crate::pull_request::DiffSide;

fn hosting(forge: Forge, host: &str) -> Hosting {
    Hosting {
        forge,
        host: host.into(),
        path: "fork/project".into(),
        web: format!("https://{host}"),
    }
}
fn ok(s: &str) -> Result<String, String> {
    Ok(s.into())
}
fn flag<'a>(call: &'a Call, name: &str) -> Option<&'a str> {
    call.args
        .windows(2)
        .find(|p| p[0] == name)
        .map(|p| p[1].as_str())
}
fn capture(
    h: &Hosting,
    answers: Vec<Result<String, String>>,
    work: impl FnOnce() -> Result<(), String>,
) -> Result<Vec<Call>, String> {
    let calls = recording::run(answers, work)?;
    assert!(!calls.is_empty());
    for call in &calls {
        assert_eq!(
            call.tool,
            if h.forge == Forge::GitHub {
                "gh"
            } else {
                "glab"
            }
        );
        if call.args[0] == "api" {
            assert_eq!(flag(call, "--hostname"), Some(h.host.as_str()), "{call:?}");
            assert_eq!(flag(call, "--repo"), None, "api does not accept --repo");
        } else {
            let repo = if h.forge == Forge::GitHub {
                format!("{}/{}", h.host, h.path)
            } else {
                format!("{}/{}", h.web, h.path)
            };
            assert_eq!(flag(call, "--repo"), Some(repo.as_str()), "{call:?}");
            assert_eq!(
                flag(call, "--hostname"),
                None,
                "repository commands do not accept --hostname"
            );
            if let Some(end) = call.args.iter().position(|a| a == "--") {
                assert!(
                    call.args
                        .iter()
                        .position(|a| a == "--repo")
                        .is_some_and(|repo| repo < end)
                );
            }
        }
        let (key, target) = if h.forge == Forge::GitHub {
            ("GH_REPO", format!("{}/{}", h.host, h.path))
        } else {
            ("GITLAB_HOST", gitlab::api_authority(h))
        };
        assert!(
            call.env
                .iter()
                .any(|(k, v)| k == key && v.as_deref() == Some(&target))
        );
        eprintln!("recorded {} {:?}", call.tool, call.args);
    }
    Ok(calls)
}
fn new_pr<'a>(reviewers: &'a [String]) -> NewPullRequest<'a> {
    NewPullRequest {
        branch: "feature",
        base: "main",
        title: "Title",
        body: "body",
        draft: false,
        reviewers,
    }
}
fn release() -> NewRelease<'static> {
    NewRelease {
        tag: "v1.2.3",
        title: "Title",
        notes: "notes\nbody",
        prerelease: false,
    }
}

#[test]
fn provider_boundary_github_every_provider_action_uses_selected_repo_and_host()
-> Result<(), Box<dyn std::error::Error>> {
    for host in ["github.com", "github.enterprise.test"] {
        let h = hosting(Forge::GitHub, host);
        let p = github::GitHub;
        let dir = std::env::temp_dir();
        capture(&h, vec![ok("[]")], || {
            p.list(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.merged(&dir, &h)?;

            Ok(())
        })?;
        let url = format!("{}/{}/pull/7", h.web, h.path);
        capture(&h, vec![ok(&url), ok("")], || {
            assert_eq!(p.create(&dir, &h, &new_pr(&["reviewer".into()]))?, url);

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.assigned_issues(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![ok(r#"{"number":7,"title":"fork issue"}"#)], || {
            assert_eq!(p.issue(&dir, &h, 7)?.title, "fork issue");

            Ok(())
        })?;
        capture(&h, vec![ok("")], || {
            p.retarget(&dir, &h, 7, "main")?;

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.find(&dir, &h, "feature")?;

            Ok(())
        })?;
        capture(&h, vec![ok("[]"), ok("[]")], || {
            p.forks(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![ok(r#"{"workflow_runs":[]}"#)], || {
            p.waiting_runs(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![ok("")], || {
            p.approve_run(&dir, &h, 42)?;

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.avatars(&dir, &h, &[]);

            Ok(())
        })?;
        capture(&h, vec![ok("[]"), ok("[]")], || {
            p.inbox(&dir, &h)?;

            Ok(())
        })?;
        let calls = capture(&h, vec![Err("not found".into()), ok("release url")], || {
            p.create_release(&dir, &h, &release())?;

            Ok(())
        })?;
        assert_eq!(calls[1].input.as_deref(), Some("notes\nbody"));
        capture(&h, vec![ok("existing release url")], || {
            p.create_release(&dir, &h, &release())?;

            Ok(())
        })?;
        capture(&h, vec![ok("")], || {
            p.upload_assets(&dir, &h, "v1.2.3", &["-asset.zip".into()])?;

            Ok(())
        })?;
    }

    Ok(())
}

#[test]
fn provider_boundary_gitlab_every_provider_action_uses_selected_repo_and_host()
-> Result<(), Box<dyn std::error::Error>> {
    for host in ["gitlab.com", "gitlab.enterprise.test"] {
        let mut h = hosting(Forge::GitLab, host);
        h.path = "fork/subgroup/project".into();
        let p = gitlab::GitLab;
        let dir = std::env::temp_dir();
        capture(&h, vec![ok("[]")], || {
            p.list(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.merged(&dir, &h)?;

            Ok(())
        })?;
        let url = format!("{}/{}/-/merge_requests/7", h.web, h.path);
        capture(&h, vec![ok(&url), ok("")], || {
            assert_eq!(p.create(&dir, &h, &new_pr(&["reviewer".into()]))?, url);

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.assigned_issues(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![ok(r#"{"iid":7,"title":"fork issue"}"#)], || {
            assert_eq!(p.issue(&dir, &h, 7)?.title, "fork issue");

            Ok(())
        })?;
        capture(&h, vec![ok("")], || {
            p.retarget(&dir, &h, 7, "main")?;

            Ok(())
        })?;
        capture(&h, vec![ok("[]")], || {
            p.find(&dir, &h, "feature")?;

            Ok(())
        })?;
        let calls = capture(&h, vec![ok("[]")], || {
            p.forks(&dir, &h)?;

            Ok(())
        })?;
        assert!(
            calls[0]
                .args
                .iter()
                .any(|s| s.starts_with("projects/fork%2Fsubgroup%2Fproject/"))
        );
        capture(&h, vec![ok("[]"), ok("[]")], || {
            p.inbox(&dir, &h)?;

            Ok(())
        })?;
        capture(&h, vec![Err("not found".into()), ok("")], || {
            p.create_release(&dir, &h, &release())?;

            Ok(())
        })?;
        capture(&h, vec![ok("")], || {
            p.create_release(&dir, &h, &release())?;

            Ok(())
        })?;
        capture(&h, vec![ok("")], || {
            p.upload_assets(&dir, &h, "v1.2.3", &["asset.zip".into()])?;

            Ok(())
        })?;
    }

    Ok(())
}

#[test]
fn provider_boundary_github_details_and_all_review_actions_keep_the_captured_fork()
-> Result<(), Box<dyn std::error::Error>> {
    let h = hosting(Forge::GitHub, "github.enterprise.test");
    let dir = std::env::temp_dir();
    let p = github::GitHub;
    let view = serde_json::json!({"number":7,"url":format!("{}/{}/pull/7",h.web,h.path),"headRefOid":"abc","headRefName":"feature"}).to_string();
    let threads = r#"{"data":{"repository":{"pullRequest":{"id":"fork-pr-node","headRefOid":"abc","headRepository":{"nameWithOwner":"fork/project"},"pending":{"nodes":[{"id":"fork-review-node"}]}}}}}"#;
    let mut details = None;
    let calls = capture(&h, vec![ok(&view), ok(threads)], || {
        details = Some(p.details(&dir, &h, 7)?);

        Ok(())
    })?;
    let payload: serde_json::Value =
        serde_json::from_str(calls[1].input.as_deref().ok_or("missing graphql payload")?)?;
    assert_eq!(payload["variables"]["owner"], "fork");
    assert_eq!(payload["variables"]["name"], "project");
    let mut details = details.ok_or("missing details")?;
    let target = p.target(&dir, &h, &details).ok_or("missing target")?;
    let line = LineComment {
        path: "a.rs".into(),
        line: 1,
        side: DiffSide::Right,
        body: "line".into(),
    };
    capture(&h, vec![ok("{}")], || target.comment("comment"))?;
    capture(&h, vec![ok("{}")], || {
        target.review(Verdict::Approve, "review")?;
        Ok(())
    })?;
    capture(&h, vec![ok("{}")], || target.comment_now(&line))?;
    capture(&h, vec![ok("{}")], || target.add_pending(&line))?;
    capture(&h, vec![ok("{}")], || {
        target.remove_pending("fork-comment-node", false)?;
        Ok(())
    })?;
    capture(&h, vec![ok("{}")], || target.discard_pending())?;
    capture(&h, vec![ok("{}")], || {
        target.reply("fork-thread-node", "reply")?;
        Ok(())
    })?;
    capture(&h, vec![ok("{}")], || {
        target.set_resolved("fork-thread-node", true)?;
        Ok(())
    })?;
    capture(&h, vec![ok("{}")], || {
        target.set_resolved("fork-thread-node", false)?;
        Ok(())
    })?;
    capture(&h, vec![ok("{}")], || {
        target.dismiss("fork-review-node", "reason")?;
        Ok(())
    })?;
    capture(
        &h,
        vec![ok("{}"), ok(r#"{"object":{"sha":"abc"}}"#), ok("{}")],
        || target.merge(MergeMethod::Squash, true),
    )?;
    details.extras.pending_review = None;
    let target = p.target(&dir, &h, &details).ok_or("missing target")?;
    capture(&h, vec![ok("{}")], || {
        target.review(Verdict::Comment, "review")?;
        Ok(())
    })?;
    let other = Hosting {
        path: "upstream/project".into(),
        ..h.clone()
    };
    assert!(
        p.target(&dir, &other, &details).is_none(),
        "same-number stale details must not create an upstream mutation target"
    );

    Ok(())
}

#[test]
fn provider_boundary_ssh_selects_cross_host_secondary_and_checks_configured_login()
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

#[test]
fn provider_boundary_ssh_rejects_untrusted_invalid_and_removed_targets_without_forge_call()
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

#[test]
fn provider_boundary_ci_reads_and_mutations_use_the_selected_project()
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

#[test]
fn provider_boundary_gitlab_port_is_kept_for_api_and_release_targets()
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

#[test]
fn provider_boundary_same_numbers_in_fork_and_upstream_do_not_change_git_transport()
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

#[test]
fn provider_boundary_release_blocks_incompatible_pushurl_before_any_cli_call()
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

#[test]
fn provider_boundary_creation_url_cannot_override_selected_repository_for_reviewers()
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

#[test]
fn review13_creation_and_review_targets_validate_the_web_instance() -> Result<(), String> {
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
                forge != Forge::GitLab,
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
