use super::*;
#[test]
fn actual_git_inventory_distinguishes_empty_proxy_and_redacts_configured_secrets()
-> Result<(), String> {
    let repo = crate::practice::create(crate::practice::Scenario::Detached)?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _owned = Cleanup(repo.parent().ok_or("No fixture root")?.into());
    git::practice_run(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/team/repo",
        ],
    )?;
    git::practice_run(
        &repo,
        &[
            "config",
            "http.proxy",
            "http://user:NEVER_SHOW_SECRET@proxy.invalid:8443/private",
        ],
    )?;
    git::practice_run(
        &repo,
        &["config", "http.sslCAInfo", "/private/NEVER_SHOW_CA_PATH"],
    )?;
    let result = inventory(&repo, Some("origin"))?;
    let text = result.lines.join("\n");
    assert!(text.contains("http://proxy.invalid:8443"));
    assert!(text.contains("custom CA location configured"));
    assert!(!text.contains("NEVER_SHOW_SECRET"));
    assert!(!text.contains("NEVER_SHOW_CA_PATH"));
    git::practice_run(&repo, &["config", "http.proxy", ""])?;
    assert_eq!(
        config(
            &repo,
            "http.proxy",
            Some("https://example.invalid/team/repo")
        )?,
        Some(String::new())
    );
    assert!(
        inventory(&repo, Some("origin"))?
            .lines
            .iter()
            .any(|line| line == "Git http.proxy: explicitly disabled")
    );
    Ok(())
}
#[test]
fn disabled_certificate_verification_cannot_be_reported_as_verified() {
    for value in ["false", "0", "off", "no"] {
        assert!(disabled(Some(value)));
    }
    assert!(!disabled(None));
    assert!(!disabled(Some("true")));
}
#[test]
fn endpoint_removes_every_secret_component() {
    assert_eq!(
        endpoint("https://user:secret@proxy.example:8443/private?token=x"),
        "https://proxy.example:8443"
    );
    assert_eq!(endpoint("!echo a-secret"), "configured (value withheld)");
}
#[test]
fn stages_name_proxy_and_ca_failures() {
    let proxy = classify(
        false,
        "CONNECT failed, response 407",
        true,
        std::time::Duration::from_secs(1),
    );
    assert!(
        proxy
            .lines
            .iter()
            .any(|s| s.contains("proxy wants a password"))
    );
    let ca = classify(
        false,
        "SSL certificate problem",
        true,
        std::time::Duration::ZERO,
    );
    assert!(ca.lines[2].ends_with("failed"));
    assert!(ca.lines[3].ends_with("not confirmed"));
}
#[test]
fn success_and_unknown_do_not_invent_identity() {
    let r = classify(
        true,
        "Authorization: secret",
        true,
        std::time::Duration::ZERO,
    );
    assert!(r.success);
    assert!(r.lines[3].contains("may be public"));
    assert!(!format!("{r:?}").contains("secret"));
    let r = classify(false, "unknown", true, std::time::Duration::ZERO);
    assert!(r.lines[0].ends_with("not confirmed"));
}
#[test]
fn ssh_destinations_are_not_shell_programs() {
    assert_eq!(
        ssh_host("git@example.org:r.git"),
        Some("git@example.org".into())
    );
    assert_eq!(ssh_host("ssh://-oProxyCommand=bad/r"), None);
    assert_eq!(ssh_host("file:///tmp/r"), None);
}
