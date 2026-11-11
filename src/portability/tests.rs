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

struct Fixture {
    root: std::path::PathBuf,
    repo: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Result<Self, String> {
        let repo = crate::practice::create(crate::practice::Scenario::Published)?;
        let root = repo.parent().ok_or("No fixture root")?.to_owned();
        Ok(Self { root, repo })
    }
    fn parent(&self) -> Result<std::path::PathBuf, String> {
        let p = self.root.join("restore-targets");
        std::fs::create_dir(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }
    fn definition(&self) -> Result<Repository, String> {
        snapshot(
            &self.repo,
            vec![1],
            vec!["refs/remotes/origin/feature".into()],
            &|| true,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

include!("tests/restore.rs");

#[test]
fn ssh_alias_repository_forces_fresh_key_setup_even_when_the_old_profile_used_an_ambient_agent()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut repository = fixture.definition()?;
    repository.remotes = vec![Remote {
        name: "origin".into(),
        fetch: "ssh://git@work-alias/team/repo.git".into(),
        push: Vec::new(),
    }];
    let mut file = Setup::new(&Settings::default(), &Default::default());
    file.repositories.push(repository);
    assert!(!file.profiles[0].setup.ssh_key_reselection_required);
    // Simulate an older/untrusted file omitting the flag instead of using encode's guard.
    let bytes = serde_json::to_vec(&file).map_err(|e| e.to_string())?;
    let decoded = Setup::decode(&bytes)?;
    let imported = decoded.profiles[0].materialize(99);
    assert!(imported.pending_setup());
    assert!(imported.connections.ssh_key.is_empty());
    assert!(
        imported
            .pending_setup
            .as_ref()
            .is_some_and(|s| s.ssh_key_reselection_required)
    );
    assert_eq!(
        decoded.repositories[0].remotes[0].fetch,
        "ssh://git@work-alias/team/repo.git"
    );
    Ok(())
}

#[test]
fn portable_preferences_round_trip_workflow_and_layout_without_executables() -> Result<(), String> {
    let original = Settings {
        offline: true,
        branch_names_keep_end: true,
        fold_patterns: "*.generated.*\npackage-lock.json".into(),
        auto_fetch_minutes: 15,
        shell: "SECRET_EXECUTABLE".into(),
        ..Default::default()
    };
    let bytes = Setup::new(&original, &Default::default()).encode()?;
    let decoded = Setup::decode(&bytes)?;
    let mut restored = Settings::default();
    decoded.preferences.apply(&mut restored);
    assert!(restored.offline);
    assert!(restored.branch_names_keep_end);
    assert_eq!(restored.fold_patterns, original.fold_patterns);
    assert_eq!(restored.auto_fetch_minutes, 15);
    assert!(restored.shell.is_empty());
    Ok(())
}

#[test]
fn linked_worktree_branch_can_resolve_from_a_secondary_remote() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut repository = fixture.definition()?;
    repository.remotes = vec![
        Remote {
            name: "origin".into(),
            fetch: "https://primary.fixture.invalid/repo".into(),
            push: Vec::new(),
        },
        Remote {
            name: "upstream".into(),
            fetch: "https://secondary.fixture.invalid/repo".into(),
            push: Vec::new(),
        },
    ];
    let commit = repository.commit.as_deref().ok_or("No fixture commit")?;
    crate::git::practice_run(
        &fixture.repo,
        &["update-ref", "refs/remotes/upstream/secondary-only", commit],
    )?;
    assert_eq!(
        restore::branch_ref(
            &fixture.repo,
            &repository.remotes,
            "secondary-only",
            &|| true
        )?,
        "refs/remotes/upstream/secondary-only"
    );
    assert!(
        restore::branch_ref(
            &fixture.repo,
            &repository.remotes[..1],
            "secondary-only",
            &|| true
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn author_host_policy_round_trips_and_legacy_profiles_default_empty() -> Result<(), String> {
    use crate::settings::profiles::HostAuthor;
    let mut settings = Settings::default();
    settings.user_profiles[0].author_hosts = vec![
        HostAuthor {
            host: "forge.example".into(),
            name: "Host author".into(),
            email: "host@example.invalid".into(),
        },
        HostAuthor {
            host: "forge.example:8443/team".into(),
            name: "Team author".into(),
            email: "team@example.invalid".into(),
        },
    ];
    for host in [
        "https://github.com/team",
        "",
        "forge.example/team/subteam",
        "[2001:db8::1]:8443/team",
    ] {
        settings.user_profiles[0].author_hosts.push(HostAuthor {
            host: host.into(),
            name: "Preserved author".into(),
            email: "preserved@example.invalid".into(),
        });
    }
    let setup = Setup::new(&settings, &Default::default());
    let decoded = Setup::decode(&setup.encode()?)?;
    assert_eq!(
        decoded.profiles[0].materialize(42).author_hosts,
        settings.user_profiles[0].author_hosts
    );
    let mut legacy = serde_json::to_value(&setup).map_err(|e| e.to_string())?;
    legacy["profiles"][0]
        .as_object_mut()
        .ok_or("profile")?
        .remove("author_hosts");
    let decoded = Setup::decode(&serde_json::to_vec(&legacy).map_err(|e| e.to_string())?)?;
    assert!(decoded.profiles[0].materialize(42).author_hosts.is_empty());
    for invalid in [
        serde_json::json!([{"host":"x","name":"n","email":"e","command":"bad"}]),
        serde_json::json!([{"host":"x\n","name":"n","email":"e"}]),
        serde_json::json!([{"host":"x","name":"n".repeat(513),"email":"e"}]),
        serde_json::Value::Array(vec![
            serde_json::json!({"host":"x","name":"n","email":"e"});
            65
        ]),
    ] {
        legacy["profiles"][0]["author_hosts"] = invalid;
        assert!(Setup::decode(&serde_json::to_vec(&legacy).map_err(|e| e.to_string())?).is_err());
    }
    Ok(())
}
