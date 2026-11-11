// Publication routes. Included to preserve libtest shard names.
fn review10_hosting() -> Hosting {
    Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "selected/project".into(),
        web: "https://github.com".into(),
    }
}

#[test]
fn review10_publishing_steps_refuse_mismatched_pushurl_before_local_mutation() {
    for step in [Step::Bump, Step::Tag] {
        let (dir, bare) = project(&format!("preflight-{step:?}"));
        let read = draft(&dir, "HEAD", Some("origin".into()));
        assert!(read.is_ok());
        let Ok(read) = read else { return };
        let mut plan = plan_of(&read, "notes");
        plan.publish = true;
        let before = git::head(&dir);
        let mut done = Done::default();
        let result = run_step(
            &dir,
            &plan,
            step,
            &mut done,
            None,
            Some(&review10_hosting()),
        );
        let head = git::head(&dir);
        let tag_absent = git::resolve(&dir, &format!("refs/tags/{}", plan.tag)).is_err();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bare);
        assert!(
            result.is_err_and(|e| e.contains("does not match the selected forge repository")),
            "{step:?} must refuse before changing the repository"
        );
        assert_eq!(head, before);
        assert!(tag_absent);
        assert_eq!(done, Done::default());
    }
}

#[test]
fn review10_push_rechecks_destination_and_preserves_completed_local_steps() {
    let (dir, bare) = project("pre-push");
    let receiver = folder("pre-push-owned-mismatched.git");
    run(&receiver, &["init", "-q", "--bare"]);
    let read = draft(&dir, "HEAD", Some("origin".into()));
    assert!(read.is_ok());
    let Ok(read) = read else { return };
    let mut plan = plan_of(&read, "notes");
    let mut done = Done::default();
    assert!(run_step(&dir, &plan, Step::Bump, &mut done, None, None).is_ok());
    assert!(run_step(&dir, &plan, Step::Tag, &mut done, None, None).is_ok());
    let before_done = done.clone();
    let before_head = git::head(&dir);
    // Even the failing-before run can push only to an owned LOCAL bare fixture.
    run(
        &dir,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            &receiver.to_string_lossy(),
        ],
    );
    assert_eq!(
        run(&dir, &["remote", "get-url", "--push", "origin"]).trim(),
        receiver.to_string_lossy()
    );
    plan.publish = true;
    let result = run_step(
        &dir,
        &plan,
        Step::Push,
        &mut done,
        None,
        Some(&review10_hosting()),
    );
    let receiver_refs = run(&receiver, &["for-each-ref", "--format=%(refname)"]);
    let head = git::head(&dir);
    let undoable = git::resolve(&dir, &format!("refs/tags/{}", plan.tag)).is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    let _ = std::fs::remove_dir_all(&receiver);
    assert!(result.is_err_and(|e| e.contains("does not match the selected forge repository")));
    assert!(
        receiver_refs.trim().is_empty(),
        "mismatched receiver must receive no refs: {receiver_refs}"
    );
    assert_eq!(done, before_done);
    assert_eq!(head, before_head);
    assert!(undoable);
}

#[test]
fn review10_destination_identity_keeps_gitlab_path_case_and_web_authority() {
    let h = Hosting {
        forge: Forge::GitLab,
        host: "gitlab.example.test".into(),
        path: "Group/Subgroup/Project".into(),
        web: "https://gitlab.example.test:8443".into(),
    };
    assert!(same_publication_repository(
        "https://GITLAB.example.test:8443/Group/Subgroup/Project.git",
        &h
    ));
    assert!(!same_publication_repository(
        "https://gitlab.example.test:8443/group/Subgroup/Project.git",
        &h
    ));
    assert!(!same_publication_repository(
        "https://gitlab.example.test:8443/Group/subgroup/Project.git",
        &h
    ));
    assert!(!same_publication_repository(
        "https://gitlab.example.test:8443/Group/Subgroup/project.git",
        &h
    ));
    assert!(!same_publication_repository(
        "https://gitlab.example.test/Group/Subgroup/Project.git",
        &h
    ));
    assert!(!same_publication_repository(
        "http://gitlab.example.test:8443/Group/Subgroup/Project.git",
        &h
    ));
    for forge in [Forge::GitHub, Forge::Forgejo, Forge::Gitea] {
        let h = Hosting {
            forge,
            host: "forge.example.test".into(),
            path: "Owner/Project".into(),
            web: "https://forge.example.test".into(),
        };
        assert!(same_publication_repository(
            "git@FORGE.example.test:owner/project.git",
            &h
        ));
        assert!(!same_publication_repository(
            "https://other.example.test/owner/project",
            &h
        ));
    }
}

#[test]
fn ssh_publication_identity_preserves_custom_web_ports_and_proxy_prefixes() {
    for forge in [Forge::GitLab, Forge::Forgejo, Forge::Gitea] {
        let hosting = Hosting {
            forge,
            host: "forge.example.test".into(),
            path: "Owner/Project".into(),
            web: "https://forge.example.test:8443/reviews".into(),
        };
        assert!(same_publication_repository(
            "ssh://git@forge.example.test:2222/Owner/Project.git",
            &hosting
        ));
        assert!(!same_publication_repository(
            "ssh://git@other.example.test:2222/Owner/Project.git",
            &hosting
        ));
        assert!(!same_publication_repository(
            "ssh://git@forge.example.test:2222/Other/Project.git",
            &hosting
        ));
    }
}

#[test]
fn review12_matching_http_single_label_publication_preflight() {
    let (dir, bare) = project("single-label-preflight");
    let read = draft(&dir, "HEAD", Some("origin".into()));
    assert!(read.is_ok(), "{read:?}");
    let Ok(read) = read else { return };
    let mut plan = plan_of(&read, "notes");
    plan.publish = true;
    let hosting = Hosting {
        forge: Forge::Forgejo,
        host: "forgejo".into(),
        path: "team/repo".into(),
        web: "http://forgejo:3000".into(),
    };
    run(
        &dir,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            "http://forgejo:3000/team/repo.git",
        ],
    );
    // Reads only Git's resolved URL; no HTTP request, push or forge process runs.
    let result = check_publication_destination(&dir, &plan, &hosting);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn review12_http_publication_identity_keeps_authority_and_proxy_prefix() {
    for forge in Forge::ALL {
        let hosting = Hosting {
            forge,
            host: "forgejo".into(),
            path: "Team/Repo".into(),
            web: "http://forgejo:3000/reviews".into(),
        };
        for matching in [
            "http://forgejo:3000/reviews/Team/Repo.git",
            "HTTP://FORGEJO:3000/reviews/Team/Repo.git",
            "http://user@forgejo:3000/reviews/Team/Repo.git",
        ] {
            assert!(
                same_publication_repository(matching, &hosting),
                "{forge:?} {matching}"
            );
        }
        for mismatch in [
            "http://forgejo:4000/reviews/Team/Repo.git",
            "http://forgejo/reviews/Team/Repo.git",
            "https://forgejo:3000/reviews/Team/Repo.git",
            "http://another:3000/reviews/Team/Repo.git",
            "http://forgejo:3000/Team/Repo.git",
            "http://forgejo:3000/other/Team/Repo.git",
            "http://forgejo:3000/REVIEWS/Team/Repo.git",
            "http://forgejo:3000/reviews/Other/Repo.git",
        ] {
            assert!(
                !same_publication_repository(mismatch, &hosting),
                "{forge:?} {mismatch}"
            );
        }
        assert_eq!(
            same_publication_repository("http://forgejo:3000/reviews/team/repo.git", &hosting),
            !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket)
        );
        let mut default_port = hosting.clone();
        default_port.web = "http://forgejo".into();
        assert!(same_publication_repository(
            "http://FORGEJO:80/Team/Repo.git",
            &default_port
        ));
    }
}
