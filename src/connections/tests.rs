use super::*;

#[test]
fn selected_accounts_are_data_not_shell_arguments() {
    let account = Account {
        host: "github.com".into(),
        user: "work-user".into(),
        config_dir: std::env::temp_dir()
            .join("work account")
            .to_string_lossy()
            .into_owned(),
    };
    assert!(account.validate().is_ok());
    for host in [
        "-Hother",
        "https://evil.example",
        "host/path",
        "host\nother",
    ] {
        assert!(
            Account {
                host: host.into(),
                ..account.clone()
            }
            .validate()
            .is_err()
        );
    }
    for user in ["--help", "x\ny", "$(touch marker)"] {
        assert!(
            Account {
                user: user.into(),
                ..account.clone()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Account {
            config_dir: "relative/path".into(),
            ..account
        }
        .validate()
        .is_err()
    );
}

#[test]
fn explicit_account_commands_remove_ambient_identity_without_changing_the_process() {
    for (tool, vars) in [("gh", GH_TOKENS), ("glab", GL_TOKENS)] {
        let account = Account {
            host: "forge.example.org".into(),
            user: "work".into(),
            config_dir: std::env::temp_dir()
                .join("account folder")
                .to_string_lossy()
                .into_owned(),
        };
        let command = account_command(tool, &account);
        assert!(command.is_ok());
        let Ok(command) = command else {
            return;
        };
        let env: std::collections::HashMap<_, _> = command.get_envs().collect();
        for key in vars {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None), "{key}");
        }
        if tool == "glab" {
            // Independent regression list: omitting these from the implementation's token
            // list must fail even if ordinary access-token scrubbing remains intact.
            for key in [
                "OAUTH2_REFRESH_TOKEN",
                "OAUTH2_EXPIRY_DATE",
                "GITLAB_CLIENT_ID",
                "CLIENT_ID",
                "JOB_TOKEN",
            ] {
                assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None), "{key}");
            }
        }
        let config = if tool == "gh" {
            "GH_CONFIG_DIR"
        } else {
            "GLAB_CONFIG_DIR"
        };
        assert_eq!(
            env.get(std::ffi::OsStr::new(config)),
            Some(&Some(std::ffi::OsStr::new(&account.config_dir)))
        );
    }
}

#[test]
fn workers_inherit_the_initiating_profile_and_nested_contexts_restore() {
    let a = Context {
        git_config: Some("profile-a.gitconfig".into()),
        choices: Choices {
            github: Account {
                host: "github.com".into(),
                user: "alice".into(),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let b = Context {
        git_config: Some("profile-b.gitconfig".into()),
        choices: Choices {
            github: Account {
                host: "github.com".into(),
                user: "bob".into(),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    a.run(|| {
        let (send, receive) = std::sync::mpsc::channel();
        let worker = spawn(move || {
            receive.recv().map_err(|e| e.to_string())?;
            Ok::<_, String>(current())
        });
        b.run(|| {
            assert_eq!(current(), b);
            assert!(send.send(()).is_ok());
        });
        assert_eq!(current(), a);
        assert!(matches!(worker.join(),Ok(Ok(context)) if context==a));
    });
}

#[test]
fn persisted_profiles_keep_connections_separate_from_author_and_signing() {
    let mut profile = crate::settings::profiles::UserProfile {
        author_name: "Author".into(),
        author_email: "author@example.org".into(),
        ..Default::default()
    };
    profile.connections.ssh_key = std::env::temp_dir()
        .join("work key")
        .to_string_lossy()
        .into_owned();
    profile.connections.github.user = "work-account".into();
    profile.connections.tea_login = "work forge".into();
    let encoded = serde_json::to_string(&profile);
    assert!(encoded.is_ok());
    let decoded = encoded
        .ok()
        .and_then(|s| serde_json::from_str::<crate::settings::profiles::UserProfile>(&s).ok());
    assert_eq!(decoded, Some(profile));
    let old = serde_json::from_str::<crate::settings::profiles::UserProfile>(
        r#"{"id":9,"name":"Legacy","author_name":"Old author"}"#,
    );
    assert!(
        matches!(old,Ok(profile) if profile.author_name=="Old author" && !profile.connections.github.selected() && profile.connections.ssh_key.is_empty())
    );
}

include!("tests/token_fixtures.rs");

#[test]
fn a_config_snapshot_survives_until_the_last_worker_releases_it() {
    let path = std::env::temp_dir().join(format!(
        "gitcraft-profile-lease-{}.gitconfig",
        std::process::id()
    ));
    let first = ConfigLease::acquire(path.clone());
    assert!(std::fs::write(&path, "[user]\n name = First\n").is_ok());
    let again = ConfigLease::acquire(path.clone());
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    drop(first);
    assert!(path.is_file());
    drop(again);
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn ssh_helper_command_quotes_executable_and_encodes_key_paths() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-quoting-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ssh = dir.join("gitcraft 'helper'");
    std::fs::write(&ssh, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE\"\n")
        .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let capture = dir.join("args");
    let key = dir.join("work 'key' $(touch INJECTED); `echo BAD` \\");
    let key = key.to_string_lossy().into_owned();
    let path = std::env::join_paths([
        dir.clone(),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ])
    .map_err(|e| e.to_string())?;
    let status = Command::new("sh")
        .args(["-c", &ssh_helper_command(&key, &ssh)])
        .current_dir(&dir)
        .env("PATH", path)
        .env("CAPTURE", &capture)
        .status()
        .map_err(|e| e.to_string())?;
    assert!(status.success());
    assert!(!dir.join("INJECTED").exists());
    assert_eq!(
        std::fs::read_to_string(capture).map_err(|e| e.to_string())?,
        format!(
            "--gitcraft-profile-ssh\n{}\n",
            URL_SAFE_NO_PAD.encode(key.as_bytes())
        )
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn ssh_effective_config_cannot_fall_back_to_another_identity() {
    assert!(
        validate_ssh_identities(
            "/tmp/work-key",
            "hostname forge.example\nidentityfile /tmp/work-key\nidentitiesonly yes\n",
            &[]
        )
        .is_ok()
    );
    assert!(
        validate_ssh_identities(
            "/tmp/work-key",
            "identityfile /tmp/work-key\nidentityfile /tmp/personal-key\n",
            &[]
        )
        .is_err()
    );
    assert!(validate_ssh_identities("/tmp/work-key", "identityfile none\n", &[]).is_err());
}

#[cfg(unix)]
#[test]
fn openssh_fixture_configuration_confirms_additive_identity_files_are_refused() -> Result<(), String>
{
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-effective-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let key = dir.join("selected key");
    std::fs::write(&key, "").map_err(|e| e.to_string())?;
    let config = dir.join("ssh_config");
    std::fs::write(
        &config,
        "Host fixture\n HostName fixture.invalid\n IdentityFile /tmp/another-profile-key\n",
    )
    .map_err(|e| e.to_string())?;
    let output = Command::new("ssh")
        .arg("-F")
        .arg(&config)
        .args(["-G", "-o", "IdentitiesOnly=yes", "-i"])
        .arg(&key)
        .arg("fixture")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(output.status.success());
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains("identityfile /tmp/another-profile-key"));
    assert!(validate_ssh_identities(&key.to_string_lossy(), &printed, &[]).is_err());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn expected_accounts_are_nonsecret_exact_host_metadata_not_transport_or_tea_aliases() {
    let context = Context {
        choices: Choices {
            github: Account {
                host: "github.com".into(),
                user: "work".into(),
                ..Default::default()
            },
            tea_login: "personal".into(),
            tea_host: "forge.example".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    context.run(|| {
        assert_eq!(expected_account("GITHUB.COM"), Some("work".into()));
        assert_eq!(expected_account("github.com:8443"), None);
        assert_eq!(expected_account("forge.example"), None);
    });
}

#[cfg(unix)]
#[test]
fn openssh_expands_equivalent_tilde_and_percent_identity_paths_but_refuses_other_keys()
-> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-expansion-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let config = dir.join("ssh_config");
    let key = dir.join("fixture.invalid-2222-git key");
    std::fs::write(&key, "").map_err(|e| e.to_string())?;
    let probe = Command::new("ssh")
        .args(["-G", "-F", "/dev/null", "-o", "ControlPath=%d", "fixture"])
        .output()
        .map_err(|e| e.to_string())?;
    assert!(probe.status.success());
    let text = String::from_utf8_lossy(&probe.stdout);
    let home = text
        .lines()
        .find_map(|line| line.strip_prefix("controlpath "))
        .ok_or("No SSH home expansion")?;
    let up = "../".repeat(
        Path::new(home)
            .components()
            .filter(|part| matches!(part, std::path::Component::Normal(_)))
            .count(),
    );
    let absolute = key.to_string_lossy();
    for identity in [
        format!("~/{up}{}", absolute.trim_start_matches('/')),
        format!("%d/{up}{}", absolute.trim_start_matches('/')),
        format!("{}/%h-%p-%r key", dir.display()),
        format!("{}/%n-%p-%r key", dir.display()),
    ] {
        std::fs::write(&config, format!("Host fixture.invalid\n HostName fixture.invalid\n User git\n Port 2222\n IdentityFile \"{identity}\"\n")).map_err(|e|e.to_string())?;
        let args = vec![
            "-F".to_owned(),
            config.to_string_lossy().into_owned(),
            "fixture.invalid".into(),
        ];
        let output = ssh_command(&absolute)
            .arg("-G")
            .args(&args)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(output.status.success());
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(printed.contains(&format!("identityfile {identity}")));
        assert_eq!(
            validate_ssh_identities(&absolute, &printed, &args),
            Ok(()),
            "{identity}"
        );
        let other = dir.join("different key");
        std::fs::write(&other, "").map_err(|e| e.to_string())?;
        assert!(validate_ssh_identities(&other.to_string_lossy(), &printed, &args).is_err());
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn fenced_ssh_expansion_preserves_the_whole_path_and_refuses_ambiguous_fields() {
    let fence = "__END__";
    assert_eq!(
        expanded_identity(
            "host fixture\ncontrolpath /tmp/selected key __END__\nidentityfile ${KEY}\n",
            fence
        ),
        Ok("/tmp/selected key ".into())
    );
    for value in [
        "/tmp/key\nother-key",
        "/tmp/key\r",
        "/tmp/key\t",
        "/tmp/key__END__\nother-key",
        "/tmp/key\ncontrolpath /tmp/key",
    ] {
        assert!(
            expanded_identity(
                &format!("host fixture\ncontrolpath {value}{fence}\nidentityfile ${{KEY}}\n"),
                fence
            )
            .is_err(),
            "{value:?}"
        );
    }
    assert!(expanded_identity("controlpath /tmp/key__END__extra\n", fence).is_err());
    assert!(validate_ssh_identities("/tmp/key", "identityfile /tmp/key\r\n", &[]).is_err());
}

include!("tests/ssh_execution.rs");

include!("tests/ssh_options.rs");
