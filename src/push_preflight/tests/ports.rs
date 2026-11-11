#[test]
fn explicit_default_ports_keep_host_and_namespace_identity_policy() -> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let mut profile = f.profile();
    profile
        .author_hosts
        .push(crate::settings::profiles::HostAuthor {
            host: "github.com/personal".into(),
            name: "Personal".into(),
            email: "personal@example.org".into(),
        });
    profile
        .author_hosts
        .push(crate::settings::profiles::HostAuthor {
            host: "github.com:443".into(),
            name: "General".into(),
            email: profile.author_email.clone(),
        });
    for url in [
        "https://github.com/personal/repo.git",
        "https://github.com:443/personal/repo.git",
    ] {
        f.git(&["remote", "set-url", "origin", url])?;
        let plan = f.plan(profile.clone())?;
        assert_eq!(plan.expected_email, "personal@example.org");
        assert!(
            plan.findings
                .iter()
                .any(|finding| finding.kind == Kind::Identity)
        );
    }
    assert_eq!(
        profile
            .expected_author("https://github.com:8443/personal/repo.git")
            .1,
        profile.author_email
    );
    profile.author_hosts[0].host = "github.com:443/personal".into();
    assert_eq!(
        profile
            .expected_author("https://github.com/personal/repo.git")
            .1,
        "personal@example.org"
    );
    assert_eq!(
        profile
            .expected_author("ssh://git@github.com:443/personal/repo.git")
            .1,
        "personal@example.org"
    );
    assert_eq!(
        profile
            .expected_author("ssh://git@github.com/personal/repo.git")
            .1,
        profile.author_email
    );
    Ok(())
}
