#[test]
fn host_and_namespace_policies_preserve_profile_serialization_and_transport_identity() {
    assert_eq!(
        target_label("file://user:DO_NOT_DISPLAY@host/path?token=DO_NOT_DISPLAY"),
        "Local repository: host/path"
    );
    let mut p = UserProfile {
        author_email: "work@example.org".into(),
        ..Default::default()
    };
    p.author_hosts.push(crate::settings::profiles::HostAuthor {
        host: "github.com".into(),
        name: "Personal".into(),
        email: "personal@example.org".into(),
    });
    p.author_hosts.push(crate::settings::profiles::HostAuthor {
        host: "github.com/company".into(),
        name: "Work".into(),
        email: "company@example.org".into(),
    });
    assert_eq!(
        p.expected_author("git@GitHub.com:personal/repo.git").1,
        "personal@example.org"
    );
    assert_eq!(
        p.expected_author("ssh://git@ssh.github.com:443/personal/repo.git")
            .1,
        "personal@example.org"
    );
    assert_eq!(
        p.expected_author("https://token:withheld@github.com/company/repo.git?token=withheld")
            .1,
        "company@example.org"
    );
    assert_eq!(
        p.expected_author("https://github.com/company-other/repo").1,
        "personal@example.org"
    );
    assert_eq!(
        p.expected_author("https://github.com:443/company/repo").1,
        "company@example.org"
    );
    let json = serde_json::to_string(&p).unwrap_or_default();
    assert_eq!(serde_json::from_str::<UserProfile>(&json).ok(), Some(p));
}

#[test]
fn gerrit_review_and_annotated_tag_pushes_keep_the_inspected_objects_and_namespaces()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = f.receiver_head()?;
    let id = "I0123456789abcdef0123456789abcdef01234567";
    let head = f.commit("review", "review", &format!("Review\n\nChange-Id: {id}"))?;
    let review = crate::gerrit::review_ops("origin", "main", "", &head, &base)?;
    let plan = scan(&f.repo, review, f.profile(), &|| true)?;
    assert_eq!(plan.destination, "refs/for/main");
    execute(&f.repo, &plan, None)?;
    assert_eq!(git::resolve(&f.receiver, "refs/for/main")?, head);
    f.git(&["tag", "-a", "-m", "Release annotation", "release"])?;
    let oid = f.git(&["rev-parse", "refs/tags/release"])?;
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
    execute(&f.repo, &plan, None)?;
    assert_eq!(
        git::run(Some(&f.receiver), &["rev-parse", "refs/tags/release"])?.trim(),
        oid
    );
    assert_eq!(
        git::run(Some(&f.receiver), &["cat-file", "-t", "refs/tags/release"])?.trim(),
        "tag"
    );
    Ok(())
}

#[test]
fn author_rules_rank_namespace_specificity_after_host_normalization() {
    use crate::settings::profiles::HostAuthor;
    let rule = |host: &str, email: &str| HostAuthor {
        host: host.into(),
        name: "Expected author".into(),
        email: email.into(),
    };
    for (scheme, host, broad) in [
        ("https", "github.com", "github.com:443"),
        ("http", "github.com", "github.com:80"),
        ("ssh", "github.com", "github.com:22"),
        ("git", "github.com", "github.com:9418"),
        ("https", "[2001:db8::1]", "[2001:db8::1]:443"),
        ("ssh", "github.com", "ssh.github.com:443"),
        ("https", "github.com", "  github.com  "),
    ] {
        let specific = rule(&format!("{host}/a"), "team@example.org");
        let general = rule(broad, "general@example.org");
        for rules in [
            vec![specific.clone(), general.clone()],
            vec![general.clone(), specific.clone()],
        ] {
            let profile = UserProfile {
                author_hosts: rules,
                ..Default::default()
            };
            assert_eq!(
                profile
                    .expected_author(&format!("{scheme}://{host}/a/repo.git"))
                    .1,
                "team@example.org",
                "{scheme}: {broad}",
            );
        }
    }
    let profile = UserProfile {
        author_email: "fallback@example.org".into(),
        author_hosts: vec![
            rule("github.com/a/sub", "nested@example.org"),
            rule("  GITHUB.COM:443/A  ", "first@example.org"),
            rule("github.com/a", "last@example.org"),
        ],
        ..Default::default()
    };
    assert_eq!(
        profile.expected_author("https://github.com/a/sub/repo").1,
        "nested@example.org"
    );
    assert_eq!(
        profile.expected_author("https://github.com/a/repo").1,
        "last@example.org"
    );
    assert_eq!(
        profile.expected_author("https://github.com/another/repo").1,
        "fallback@example.org"
    );
    assert_eq!(
        profile.expected_author("https://github.com:8443/a/repo").1,
        "fallback@example.org"
    );
}

#[test]
fn author_rules_normalize_namespace_directory_suffixes() {
    use crate::settings::profiles::HostAuthor;
    let rule = |host: String, email: &str| HostAuthor {
        host,
        name: "Expected author".into(),
        email: email.into(),
    };
    for (scheme, host, authority) in [
        ("https", "github.com", "github.com:443"),
        ("http", "github.com", "github.com:80"),
        ("ssh", "github.com", "github.com:22"),
        ("git", "github.com", "github.com:9418"),
        ("https", "[2001:db8::1]", "[2001:db8::1]:443"),
        ("ssh", "github.com", "ssh.github.com:443"),
    ] {
        let parent = rule(format!(" {authority}/COMPANY/////// "), "team@example.org");
        let nested = rule(format!("{host}/company/sub/"), "nested@example.org");
        for specific in [
            vec![parent.clone(), nested.clone()],
            vec![nested.clone(), parent.clone()],
        ] {
            let mut rules = vec![rule(format!("{host}////"), "general@example.org")];
            rules.extend(specific);
            let profile = UserProfile {
                author_email: "fallback@example.org".into(),
                author_hosts: rules,
                ..Default::default()
            };
            let stored = profile.author_hosts.clone();
            for (path, expected) in [
                ("company/repo", "team@example.org"),
                ("company", "team@example.org"),
                ("company/sub/repo", "nested@example.org"),
                ("company-other/repo", "general@example.org"),
            ] {
                assert_eq!(
                    profile
                        .expected_author(&format!("{scheme}://{host}/{path}"))
                        .1,
                    expected
                );
            }
            assert_eq!(profile.author_hosts, stored);
        }
        for (first, last) in [("company", "company/"), ("company///", "company")] {
            let profile = UserProfile {
                author_hosts: vec![
                    rule(format!("{authority}/{first}"), "first@example.org"),
                    rule(format!("{host}/{last}"), "last@example.org"),
                ],
                ..Default::default()
            };
            assert_eq!(
                profile
                    .expected_author(&format!("{scheme}://{host}/company/repo"))
                    .1,
                "last@example.org"
            );
        }
    }
}

#[test]
fn trailing_namespace_policy_controls_actual_preflight_identity_findings() -> Result<(), String> {
    use crate::settings::profiles::HostAuthor;
    for email in ["team@example.org", "fixture@work.example"] {
        let f = Fixture::new()?;
        let receiver_before = f.receiver_head()?;
        f.git(&["config", "user.name", "Team"])?;
        f.git(&["config", "user.email", email])?;
        f.commit("next", "next", "Next")?;
        let mut profile = f.profile();
        profile.author_hosts = vec![
            HostAuthor {
                host: "github.com:443".into(),
                name: "General".into(),
                email: profile.author_email.clone(),
            },
            HostAuthor {
                host: "github.com/company///".into(),
                name: "Team".into(),
                email: "team@example.org".into(),
            },
        ];
        f.git(&[
            "remote",
            "set-url",
            "origin",
            "https://github.com/company/repo.git",
        ])?;
        let plan = f.plan(profile)?;
        assert_eq!(plan.expected_name, "Team");
        assert_eq!(plan.expected_email, "team@example.org");
        assert_eq!(
            plan.findings
                .iter()
                .any(|finding| finding.kind == Kind::Identity),
            email != "team@example.org"
        );
        assert_eq!(f.receiver_head()?, receiver_before);
    }
    Ok(())
}
