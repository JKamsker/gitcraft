use super::*;

#[test]
fn portable_profiles_keep_public_policy_but_never_export_auth_paths_or_commands()
-> Result<(), String> {
    let mut settings = Settings::default();
    let profile = &mut settings.user_profiles[0];
    profile.author_name = "Public author".into();
    profile.connections.ssh_key = "/fixture/PRIVATE_SSH_PATH".into();
    profile.connections.github.user = "work-account".into();
    profile.connections.github.config_dir = "/fixture/SECRET_AUTH_FOLDER".into();
    profile.own_signing = true;
    profile.signing.key = "PRIVATE_KEY_VALUE".into();
    profile.signing.program = "EXECUTABLE_SIGNER".into();
    profile.signing.sign_commits = true;
    settings.shell = "UNTRUSTED_SHELL".into();
    settings.editor = "UNTRUSTED_EDITOR".into();
    let setup = Setup::new(&settings, &Default::default());
    let bytes = setup.encode()?;
    let text = String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?;
    for excluded in [
        "PRIVATE_SSH_PATH",
        "SECRET_AUTH_FOLDER",
        "PRIVATE_KEY_VALUE",
        "EXECUTABLE_SIGNER",
        "UNTRUSTED_SHELL",
        "UNTRUSTED_EDITOR",
        "config_dir",
    ] {
        assert!(!text.contains(excluded));
    }
    assert!(text.contains("work-account"));
    let decoded = Setup::decode(&bytes)?;
    let restored = decoded.profiles[0].materialize(99);
    assert!(restored.pending_setup());
    assert!(restored.connections.ssh_key.is_empty());
    assert!(!restored.connections.github.selected());
    assert!(restored.signing.key.is_empty());
    assert!(restored.signing.program.is_empty());
    Ok(())
}

#[test]
fn malicious_setup_fields_and_command_urls_are_data_errors() -> Result<(), String> {
    let setup = Setup::new(&Settings::default(), &Default::default());
    let mut value = serde_json::to_value(&setup).map_err(|e| e.to_string())?;
    value["raw_git_config"] = serde_json::json!({"core.sshCommand":"UNTRUSTED_COMMAND"});
    assert!(Setup::decode(&serde_json::to_vec(&value).map_err(|e| e.to_string())?).is_err());
    for url in [
        "ext::sh -c bad",
        "file:///outside",
        "http://plain.invalid/x",
        "https://-bad.invalid/x",
        "ssh://git@host.invalid:0/path",
        "git@host.invalid:../outside",
    ] {
        assert!(sanitize_url(url).is_err(), "{url}");
    }
    assert_eq!(
        sanitize_url("https://name:SECRET@host.invalid/team/repo?token=SECRET#SECRET")?.0,
        "https://host.invalid/team/repo"
    );
    assert_eq!(
        sanitize_url("ssh://git:SECRET@host.invalid:2222/team/repo")?.0,
        "ssh://git@host.invalid:2222/team/repo"
    );
    Ok(())
}

#[test]
fn signing_safety_cannot_be_disabled_by_a_missing_pending_flag() {
    let mut p = PublicProfile::capture(&UserProfile::default(), &ThemeId::default());
    p.own_signing = true;
    p.sign_commits = true;
    let profile = p.materialize(7);
    assert!(profile.pending_setup());
    assert!(
        profile
            .pending_setup
            .as_ref()
            .is_some_and(|p| p.signing_key_reselection_required)
    );
}
