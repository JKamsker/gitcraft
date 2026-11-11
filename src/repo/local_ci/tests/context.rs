#[test]
fn context_is_displayed_bound_and_revalidated_without_launching_a_child() {
    use super::context::Context;
    use crate::git::Remote;
    let remote = |url: &str| {
        vec![Remote {
            name: "fork".into(),
            url: url.into(),
        }]
    };
    assert!(Context::capture("missing", &remote("https://github.com/a/b")).is_err());
    for url in [
        "https://enterprise.example/a/b",
        "https://user:secret@github.com/a/b",
        "https://github.com:443/a/b",
        "file:///tmp/a",
        "https://github.com/a/b?token=secret",
        "https://github.com/a/b/c",
    ] {
        assert!(Context::capture("fork", &remote(url)).is_err(), "{url}");
    }
    for url in [
        "https://github.com/a/b.git",
        "git@github.com:a/b.git",
        "ssh://git@github.com/a/b.git",
    ] {
        let context = Context::capture("fork", &remote(url)).unwrap();
        assert_eq!(context.repository, "a/b");
        assert!(context.display().contains("host github.com"));
        assert!(context.display().contains(url));
        let mut calls = Vec::new();
        context
            .revalidate_with(|args| {
                calls.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                Ok(format!("{url}\n"))
            })
            .unwrap();
        assert_eq!(
            calls,
            vec![vec![
                "config",
                "--local",
                "--no-includes",
                "--get-all",
                "remote.fork.url"
            ]]
        );
        assert!(
            context
                .revalidate_with(|_| Ok("https://github.com/other/repo\n".into()))
                .is_err()
        );
        assert!(
            context
                .revalidate_with(|_| Ok(format!("{url}\n{url}\n")))
                .is_err()
        );
        assert!(
            context
                .revalidate_with(|_| Err("credential secret must not escape".into()))
                .unwrap_err()
                .find("secret")
                .is_none()
        );
        let mut s = selection();
        s.context = context;
        let args = run::arguments(
            std::path::Path::new("/worktree"),
            std::path::Path::new("/private"),
            &s,
        )
        .unwrap();
        assert!(
            args.windows(2)
                .any(|a| a == ["--github-instance", "github.com"])
        );
        assert!(
            args.windows(2)
                .any(|a| a == ["--env", "GITHUB_REPOSITORY=a/b"])
        );
        assert!(
            args.windows(2)
                .any(|a| a == ["--env", "GITHUB_REPOSITORY_OWNER=a"])
        );
    }
}

#[test]
fn both_ci_entry_paths_default_to_push_remote_and_preserve_override() {
    let ctx = egui::Context::default();
    let mut tab = super::RepoTab::new_deferred(
        std::path::PathBuf::from("/nonexistent-ci-context-test"),
        &ctx,
    );
    tab.refs.current = Some("main".into());
    tab.refs.push_remotes.insert("main".into(), "fork".into());
    tab.remotes = vec![
        crate::git::Remote {
            name: "origin".into(),
            url: "https://github.com/upstream/repo".into(),
        },
        crate::git::Remote {
            name: "fork".into(),
            url: "https://github.com/my/repo".into(),
        },
    ];
    tab.apply(
        super::Action::OpenCi {
            sha: "head".into(),
            run: None,
        },
        &ctx,
    );
    assert_eq!(tab.local_ci.remote, "fork");
    tab.local_ci.remote = "origin".into();
    tab.local_ci.remote_override = true;
    tab.apply(super::Action::LocalCi(super::Act::Open), &ctx);
    assert_eq!(tab.local_ci.remote, "origin");
    tab.local_ci.remote_override = false;
    tab.apply(super::Action::LocalCi(super::Act::Open), &ctx);
    assert_eq!(tab.local_ci.remote, "fork");
}

#[test]
fn stdout_and_stderr_callbacks_update_visible_log_before_any_result() {
    let log = std::sync::Arc::new(std::sync::Mutex::new(run::Log::default()));
    let mut options = run::options(&selection(), &log, &egui::Context::default());
    assert_eq!(options.output_limit, 8 << 20);
    options.stdout.as_mut().unwrap()(b"step one is running\n");
    assert_eq!(log.lock().unwrap().text, "step one is running\n");
    options.stderr.as_mut().unwrap()(b"diagnostic\n");
    assert_eq!(
        log.lock().unwrap().text,
        "step one is running\ndiagnostic\n"
    );
    options.stdout.as_mut().unwrap()(&vec![b'x'; 3 << 20]);
    let log = log.lock().unwrap();
    assert!(log.truncated);
    assert!(log.text.len() <= 2 << 20);
}

#[test]
fn changed_displayed_context_refuses_run_before_any_worker_submission() {
    let ctx = egui::Context::default();
    let mut tab = super::RepoTab::new_deferred(
        std::path::PathBuf::from("/nonexistent-ci-stale-context"),
        &ctx,
    );
    tab.remotes = vec![crate::git::Remote {
        name: "fork".into(),
        url: "https://github.com/previous/repo.git".into(),
    }];
    tab.local_ci.remote = "fork".into();
    tab.local_ci.displayed_context =
        Some(super::context::Context::capture("fork", &tab.remotes).unwrap());
    tab.local_ci.trust = true;
    tab.remotes[0].url = "https://github.com/replacement/repo.git".into();
    tab.apply(super::Action::LocalCi(super::Act::Run), &ctx);
    assert!(!tab.local_ci.trust);
    assert!(tab.local_ci.pending.is_none());
    assert!(
        tab.local_ci
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("changed since display")
    );
}
