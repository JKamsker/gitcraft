use super::*;

#[test]
fn selected_accounts_are_data_not_shell_arguments() {
    let account = Account {
        host: "github.com".into(),
        user: "work-user".into(),
        config_dir: "/tmp/work account".into(),
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

#[cfg(unix)]
mod fixture {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    struct Programs {
        directory: PathBuf,
    }
    impl Programs {
        fn new(name: &str, body: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "gitcraft-profile-accounts-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&directory);
            assert!(std::fs::create_dir_all(&directory).is_ok());
            for tool in ["gh", "glab"] {
                let path = directory.join(tool);
                assert!(std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).is_ok());
                assert!(
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).is_ok()
                );
                TEST_PROGRAMS.with(|map| {
                    map.borrow_mut().insert(tool.into(), path);
                });
            }
            Self { directory }
        }
    }
    impl Drop for Programs {
        fn drop(&mut self) {
            TEST_PROGRAMS.with(|map| map.borrow_mut().clear());
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    fn account(user: &str) -> Account {
        Account {
            host: "github.com".into(),
            user: user.into(),
            ..Default::default()
        }
    }

    #[test]
    fn gh_tokens_are_chosen_by_username_and_only_installed_for_the_bound_host() {
        let _programs = Programs::new(
            "github",
            r#"
if test "$1 $2" = 'auth token'; then
  test "$3 $4 $5" = '--hostname github.com --user' || exit 11
  test -z "${GH_TOKEN+x}${GITHUB_TOKEN+x}${GH_ENTERPRISE_TOKEN+x}${GITHUB_ENTERPRISE_TOKEN+x}" || exit 12
  printf 'fixture-%s\n' "$6"
else exit 13
fi"#,
        );
        let ctx = Context {
            choices: Choices {
                github: account("alice"),
                ..Default::default()
            },
            ..Default::default()
        };
        ctx.run(|| {
            let mut command = Command::new("fixture");
            assert!(configure_cli(&mut command, "gh", "github.com").is_ok());
            let token = command
                .get_envs()
                .find(|(key, _)| *key == "GH_TOKEN")
                .and_then(|(_, value)| value);
            assert_eq!(token, Some(std::ffi::OsStr::new("fixture-alice")));
            let mut wrong = Command::new("fixture");
            assert!(configure_cli(&mut wrong, "gh", "evil.example").is_err());
            assert!(wrong.get_envs().next().is_none());
            let reply = credential_reply(
                "gh",
                &account("bob"),
                "get",
                "protocol=https\nhost=github.com\n\n",
            );
            assert_eq!(
                reply,
                Ok(Some("username=bob\npassword=fixture-bob\n\n".into()))
            );
        });
    }

    #[test]
    fn a_selected_cli_account_is_routed_to_the_explicit_repository_outside_its_config()
    -> Result<(), String> {
        let programs = Programs::new(
            "repository-route",
            r#"
if test "$1 $2" = 'auth token'; then printf 'fixture-%s\n' "$6"; exit; fi
if test "$1 $2" = 'pr list'; then
 test "$GH_TOKEN" = fixture-alice || exit 31
 test "$GH_REPO" = github.com/team/app || exit 32
 test ! -f .glab-cli/config.yml || exit 33
 printf '[{"number":1}]\n'; exit
fi
exit 34"#,
        );
        let repo = programs.directory.join("repo");
        std::fs::create_dir_all(repo.join(".glab-cli")).map_err(|e| e.to_string())?;
        std::fs::write(
            repo.join(".glab-cli/config.yml"),
            "api_host: evil.example\n",
        )
        .map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q"],
            vec!["remote", "add", "origin", "https://github.com/team/app"],
        ] {
            let status = Command::new("git")
                .current_dir(&repo)
                .args(args)
                .status()
                .map_err(|e| e.to_string())?;
            assert!(status.success());
        }
        let own = programs.directory.join("global.gitconfig");
        std::fs::write(&own, "").map_err(|e| e.to_string())?;
        let context = Context {
            git_config: Some(own),
            choices: Choices {
                github: account("alice"),
                ..Default::default()
            },
            ..Default::default()
        };
        let answer =
            context.run(|| crate::forge::tool(crate::forge::Forge::GitHub, &repo, &["pr", "list"]));
        assert_eq!(answer, Ok("[{\"number\":1}]\n".into()));
        Ok(())
    }

    #[test]
    fn glab_verifies_the_named_owner_before_using_a_resolved_token() {
        let _programs = Programs::new(
            "gitlab",
            r#"
case "$1 $2 $3" in
 'config get token') test -z "${GITLAB_TOKEN+x}${GITLAB_ACCESS_TOKEN+x}${OAUTH_TOKEN+x}" || exit 21; printf 'fixture-token\n';;
 'config get is_oauth2') printf 'true\n';;
 'config get user') printf 'alice\n';;
 'api user --hostname') test "${GITLAB_TOKEN-fixture-token}" = fixture-token || exit 22; test "${GLAB_IS_OAUTH2-true}" = true || exit 23; printf '{"username":"alice"}\n';;
 *) exit 24;;
esac"#,
        );
        let selected = Account {
            host: "gitlab.com".into(),
            user: "alice".into(),
            ..Default::default()
        };
        assert!(token("glab", &selected).is_ok());
        let wrong = Account {
            user: "bob".into(),
            ..selected.clone()
        };
        assert!(token("glab", &wrong).is_err());
        assert!(
            credential_reply("glab", &wrong, "get", "protocol=https\nhost=gitlab.com\n").is_err()
        );
    }

    #[test]
    fn oauth_refresh_is_selected_then_the_exact_replacement_is_verified_and_pinned()
    -> Result<(), String> {
        let programs = Programs::new(
            "oauth-refresh",
            r#"
case "$1 $2 $3" in
 'config get token') cat "$GLAB_CONFIG_DIR/token";;
 'config get is_oauth2') printf 'true\n';;
 'config get user') printf 'alice\n';;
 'api user --hostname')
  if test -z "${GITLAB_TOKEN+x}"; then
   # Initial CLI-config refresh cannot inherit foreign OAuth/access metadata.
   test -z "${OAUTH2_REFRESH_TOKEN+x}${OAUTH2_EXPIRY_DATE+x}${JOB_TOKEN+x}" || exit 51
   printf 'fixture-refreshed\n' > "$GLAB_CONFIG_DIR/token"
   printf 'refresh\n' >> "$GLAB_CONFIG_DIR/events"
  else
   test "$GITLAB_TOKEN" = fixture-refreshed || exit 52
   test "$OAUTH2_REFRESH_TOKEN" = gitcraft-immutable-access-token || exit 53
   test "$OAUTH2_EXPIRY_DATE" = 9999-12-31T23:59:59Z || exit 54
   printf 'verify-exact\n' >> "$GLAB_CONFIG_DIR/events"
  fi
  printf '{"username":"alice"}\n';;
 'fixture-operation  ')
  test "$GITLAB_TOKEN" = fixture-refreshed || exit 55
  test "$OAUTH2_REFRESH_TOKEN" = gitcraft-immutable-access-token || exit 56
  test "$OAUTH2_EXPIRY_DATE" = 9999-12-31T23:59:59Z || exit 57
  printf 'write-with-verified-token\n' >> "$GLAB_CONFIG_DIR/events";;
 *) exit 58;;
esac"#,
        );
        let selected = Account {
            host: "gitlab.com".into(),
            user: "alice".into(),
            config_dir: programs.directory.to_string_lossy().into_owned(),
        };
        let events = programs.directory.join("events");
        let reset = || -> Result<(), String> {
            std::fs::write(programs.directory.join("token"), "fixture-expired\n")
                .map_err(|e| e.to_string())?;
            std::fs::write(&events, "").map_err(|e| e.to_string())
        };
        reset()?;
        let context = Context {
            choices: Choices {
                gitlab: selected.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        context.run(|| -> Result<(), String> {
            let mut cmd = Command::new(program("glab"));
            configure_cli(&mut cmd, "glab", "gitlab.com")?;
            assert!(
                cmd.arg("fixture-operation")
                    .status()
                    .map_err(|e| e.to_string())?
                    .success()
            );
            Ok(())
        })?;
        assert_eq!(
            std::fs::read_to_string(&events).map_err(|e| e.to_string())?,
            "refresh\nverify-exact\nwrite-with-verified-token\n"
        );
        reset()?;
        assert_eq!(
            credential_reply(
                "glab",
                &selected,
                "get",
                "protocol=https\nhost=gitlab.com\n"
            )?,
            Some("username=alice\npassword=fixture-refreshed\n\n".into())
        );
        assert_eq!(
            std::fs::read_to_string(&events).map_err(|e| e.to_string())?,
            "refresh\nverify-exact\n"
        );
        reset()?;
        assert_eq!(check_account("glab", &selected)?, "gitlab.com as alice");
        assert_eq!(
            std::fs::read_to_string(&events).map_err(|e| e.to_string())?,
            "refresh\nverify-exact\nverify-exact\n"
        );
        Ok(())
    }

    #[test]
    fn oauth_refresh_or_concurrent_config_changes_cannot_substitute_an_unverified_owner() {
        let programs = Programs::new(
            "oauth-owner",
            r#"
case "$1 $2 $3" in
 'config get token') printf 'fixture-bob-token\n';;
 'config get is_oauth2') printf 'true\n';;
 'config get user') printf 'alice\n';;
 'api user --hostname')
  if test -z "${GITLAB_TOKEN+x}"; then
   test -z "${OAUTH2_REFRESH_TOKEN+x}${OAUTH2_EXPIRY_DATE+x}" || exit 61
   printf '{"username":"alice"}\n'
  else
   # Config changed between refresh and re-read: immutable verification refuses Bob.
   test "$GITLAB_TOKEN" = fixture-bob-token || exit 62
   printf '{"username":"bob"}\n'
  fi;;
 *) printf 'UNEXPECTED-WRITE' > "$GLAB_CONFIG_DIR/wrote"; exit 63;;
esac"#,
        );
        let selected = Account {
            host: "gitlab.com".into(),
            user: "alice".into(),
            config_dir: programs.directory.to_string_lossy().into_owned(),
        };
        assert!(token("glab", &selected).is_err());
        assert!(
            credential_reply(
                "glab",
                &selected,
                "get",
                "protocol=https\nhost=gitlab.com\n"
            )
            .is_err()
        );
        assert!(check_account("glab", &selected).is_err());
        assert!(!programs.directory.join("wrote").exists());
    }

    #[test]
    fn oauth_account_metadata_mismatch_refuses_before_refresh_or_any_write() {
        let programs = Programs::new(
            "oauth-metadata-owner",
            r#"
case "$1 $2 $3" in
 'config get token') printf 'fixture-expired\n';;
 'config get is_oauth2') printf 'true\n';;
 'config get user') printf 'bob\n';;
 *) printf 'UNEXPECTED-REFRESH-OR-WRITE' > "$GLAB_CONFIG_DIR/refreshed"; exit 71;;
esac"#,
        );
        let selected = Account {
            host: "gitlab.com".into(),
            user: "alice".into(),
            config_dir: programs.directory.to_string_lossy().into_owned(),
        };
        assert!(token("glab", &selected).is_err());
        assert!(
            credential_reply(
                "glab",
                &selected,
                "get",
                "protocol=https\nhost=gitlab.com\n"
            )
            .is_err()
        );
        assert!(!programs.directory.join("refreshed").exists());
    }

    #[test]
    fn no_secret_is_resolved_for_another_protocol_server_or_store_operation() {
        let _programs = Programs::new("not-called", "exit 99");
        for (operation, input) in [
            ("get", "protocol=http\nhost=github.com\n"),
            ("get", "protocol=https\nhost=github.evil.example\n"),
            (
                "store",
                "protocol=https\nhost=github.com\npassword=fixture-secret\n",
            ),
            ("erase", "protocol=https\nhost=github.com\n"),
        ] {
            assert_eq!(
                credential_reply("gh", &account("alice"), operation, input),
                Ok(None)
            );
        }
    }

    #[test]
    fn token_cli_failures_never_echo_potential_secrets_in_errors() {
        let _programs = Programs::new(
            "failure",
            "printf 'fixture-sensitive-output\\n'; printf 'fixture-sensitive-error\\n' >&2; exit 1",
        );
        let error = token("gh", &account("alice")).err().unwrap_or_default();
        assert!(!error.is_empty());
        assert!(!error.contains("sensitive"));
    }
}

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

/// A subprocess entry exercises precisely run_ssh, including its final transport launch.
/// Ordinary test runs do nothing here; only the fixture parent supplies this metadata.
#[cfg(unix)]
#[test]
fn profile_ssh_fixture_child() -> Result<(), String> {
    let Some(request) = std::env::var_os("GITCRAFT_SSH_HELPER_FIXTURE") else {
        return Ok(());
    };
    let value: serde_json::Value =
        serde_json::from_str(&request.to_string_lossy()).map_err(|e| e.to_string())?;
    let key = value["key"].as_str().ok_or("missing fixture key")?;
    let bytes = std::fs::read(value["args_file"].as_str().ok_or("missing fixture argv")?)
        .map_err(|e| e.to_string())?;
    let args = bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8(s.to_vec()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let result = run_ssh(key, &args);
    if value["relay"].as_bool() == Some(true) {
        match result {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("SSH fixture: {error}");
                std::process::exit(1);
            }
        }
    }
    assert_eq!(
        result.is_ok(),
        value["expect"].as_bool().unwrap_or(false),
        "{result:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn actual_ssh_helper_refuses_lf_cr_and_different_keys_before_transport_and_accepts_equivalent_paths()
-> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-controls-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let key = dir.join("fixture.invalid 'quoted' \\ key");
    let fixture_key = |path: &Path| -> Result<(), String> {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}.pub", path.display()));
        let status = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Could not create an isolated SSH test key".into());
        }
        Ok(())
    };
    fixture_key(&key)?;
    let config = dir.join("ssh_config");
    let marker = dir.join("transport-reached");
    let args_file = dir.join("args");
    let proxy = format!(
        "printf reached > {}; exec false",
        shell_quote(&marker.to_string_lossy())
    );
    let args = vec![
        "-F".to_owned(),
        config.to_string_lossy().into_owned(),
        "-o".into(),
        "IdentityAgent=none".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "GlobalKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        format!("ProxyCommand={proxy}"),
        "fixture".into(),
    ];
    let mut argv = Vec::new();
    for arg in &args {
        argv.extend_from_slice(arg.as_bytes());
        argv.push(0);
    }
    std::fs::write(&args_file, argv).map_err(|e| e.to_string())?;
    let probe = Command::new("ssh")
        .args(["-G", "-F", "/dev/null", "-o", "ControlPath=%d", "fixture"])
        .output()
        .map_err(|e| e.to_string())?;
    assert!(probe.status.success());
    let probe = String::from_utf8_lossy(&probe.stdout);
    let home = probe
        .lines()
        .find_map(|line| line.strip_prefix("controlpath "))
        .ok_or("missing SSH fixture home")?;
    let up = "../".repeat(
        Path::new(home)
            .components()
            .filter(|p| matches!(p, std::path::Component::Normal(_)))
            .count(),
    );
    let absolute = key.to_string_lossy().into_owned();
    for (identity, value, expected) in [
        ("${FIXTURE_IDENTITY}".into(), absolute.clone(), true),
        (
            "${FIXTURE_IDENTITY}".into(),
            format!("{absolute}\nother-key"),
            false,
        ),
        ("${FIXTURE_IDENTITY}".into(), format!("{absolute}\r"), false),
        ("${FIXTURE_IDENTITY}".into(), format!("{absolute} "), false),
        (
            "${FIXTURE_IDENTITY}".into(),
            dir.join("other-key").to_string_lossy().into_owned(),
            false,
        ),
        (
            format!("~/{up}{}", absolute.trim_start_matches('/')),
            absolute.clone(),
            true,
        ),
        (
            absolute.replace("fixture.invalid", "%h"),
            absolute.clone(),
            true,
        ),
    ] {
        // These valid generated test keys have no account/server; unusual Unix names
        // must be refused before the recording ProxyCommand can start any transport.
        if !expected {
            fixture_key(Path::new(&value))?;
        }
        let escaped = identity.replace('\\', "\\\\").replace('"', "\\\"");
        std::fs::write(
            &config,
            format!("Host fixture\n HostName fixture.invalid\n IdentityFile \"{escaped}\"\n"),
        )
        .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&marker);
        let request = serde_json::json!({"key":absolute,"args_file":args_file,"expect":expected});
        let output = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "--exact",
                "connections::tests::profile_ssh_fixture_child",
                "--nocapture",
            ])
            .env("GITCRAFT_SSH_HELPER_FIXTURE", request.to_string())
            .env("FIXTURE_IDENTITY", &value)
            .env("PATH", "/usr/bin:/bin")
            .env_remove("SSH_AUTH_SOCK")
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            output.status.success(),
            "case {identity:?} {value:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(marker.exists(), expected, "{identity:?} {value:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(unix)]
#[test]
fn actual_profile_ssh_wrapper_auto_probe_preserves_nondefault_port_over_inherited_plink()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-real-port-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .map_err(|e| e.to_string())?
            .success()
    );
    let key = dir.join("selected key");
    std::fs::write(&key, "").map_err(|e| e.to_string())?;
    let marker = dir.join("actual-port");
    let ssh_config = dir.join("ssh_config");
    let proxy = format!(
        "sh -c {}",
        shell_quote(&format!(
            "echo %p > {}; exec false",
            shell_quote(&marker.to_string_lossy())
        ))
    );
    std::fs::write(&ssh_config,format!("Host *\n HostName fixture.invalid\n IdentityAgent none\n UserKnownHostsFile /dev/null\n GlobalKnownHostsFile /dev/null\n BatchMode yes\n ProxyCommand {proxy}\n")).map_err(|e|e.to_string())?;
    let args_file = dir.join("args");
    let wrapper = dir.join("profile-ssh-wrapper");
    std::fs::write(&wrapper,format!("#!/bin/sh\nprintf '%s\\0' '-F' {} \"$@\" > {}\n{} --exact connections::tests::profile_ssh_fixture_child --nocapture >/dev/null\nexit $?\n",shell_quote(&ssh_config.to_string_lossy()),shell_quote(&args_file.to_string_lossy()),shell_quote(&std::env::current_exe().map_err(|e|e.to_string())?.to_string_lossy()))).map_err(|e|e.to_string())?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let inherited = dir.join("user.gitconfig");
    std::fs::write(&inherited, "[ssh]\n variant = plink\n").map_err(|e| e.to_string())?;
    let profile_file = dir.join("profile.gitconfig");
    let mut profile = crate::settings::profiles::UserProfile::default();
    profile.connections.ssh_key = key.to_string_lossy().into_owned();
    let mut text = crate::settings::profiles::config_text(&profile, &[inherited]);
    // Test executable relays to the exact production helper body; no SSH imitation.
    text.push_str(&format!(
        "[core]\n sshCommand = {}\n",
        serde_json::to_string(&shell_quote(&wrapper.to_string_lossy()))
            .map_err(|e| e.to_string())?
    ));
    std::fs::write(&profile_file, text).map_err(|e| e.to_string())?;
    let request = serde_json::json!({"key":key,"args_file":args_file,"relay":true});
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let probe_marker = dir.join("ssh-was-started");
    let real_ssh = crate::shell_path::resolve("ssh");
    std::fs::write(
        bin.join("ssh"),
        format!(
            "#!/bin/sh\nprintf called > {}\nexec {} \"$@\"\n",
            shell_quote(&probe_marker.to_string_lossy()),
            shell_quote(&real_ssh)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(bin.join("ssh"), std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let path = std::env::join_paths([bin, PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
        .map_err(|e| e.to_string())?;
    for source in ["local", "environment"] {
        for variant in ["auto", "ssh", "simple", "plink", "putty", "tortoiseplink"] {
            let _ = std::fs::remove_file(&marker);
            let _ = std::fs::remove_file(&args_file);
            let _ = std::fs::remove_file(&probe_marker);
            let mut setting = Command::new("git");
            setting
                .current_dir(&repo)
                .env("GIT_CONFIG_GLOBAL", &profile_file)
                .env("GIT_CONFIG_NOSYSTEM", "1");
            if source == "local" {
                assert!(
                    setting
                        .args(["config", "--local", "ssh.variant", variant])
                        .status()
                        .map_err(|e| e.to_string())?
                        .success()
                );
            } else {
                let _ = setting
                    .args(["config", "--local", "--unset-all", "ssh.variant"])
                    .status();
            }
            let mut command = Command::new("git");
            command
                .current_dir(&repo)
                .args(["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"])
                .env("GIT_CONFIG_GLOBAL", &profile_file)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_COUNT", "0")
                .env("GITCRAFT_SSH_HELPER_FIXTURE", request.to_string())
                .env("PATH", &path)
                .env_remove("GIT_SSH_COMMAND")
                .env_remove("GIT_SSH")
                .env_remove("SSH_AUTH_SOCK");
            if source == "environment" {
                command.env("GIT_SSH_VARIANT", variant);
            } else {
                command.env_remove("GIT_SSH_VARIANT");
            }
            let output = command.output().map_err(|e| e.to_string())?;
            assert!(!output.status.success()); // all recording ProxyCommands refuse transport
            if matches!(variant, "auto" | "ssh") {
                assert_eq!(
                    std::fs::read_to_string(&marker).map_err(|e| format!(
                        "{source}/{variant}: {e}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))?,
                    "2222\n"
                );
                let args = std::fs::read(&args_file).map_err(|e| e.to_string())?;
                let args = args
                    .split(|b| *b == 0)
                    .filter_map(|s| std::str::from_utf8(s).ok())
                    .collect::<Vec<_>>();
                assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
                assert!(!args.contains(&"-P"));
            } else {
                assert!(
                    !marker.exists(),
                    "{source}/{variant}: incompatible dialect reached a transport"
                );
                assert!(
                    !probe_marker.exists(),
                    "{source}/{variant}: incompatible dialect reached an SSH probe"
                );
                assert_eq!(
                    args_file.exists(),
                    variant != "simple",
                    "{source}/{variant}: Git/helper boundary control"
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn profile_openssh_options_refuse_plink_dialects_before_any_probe() {
    for args in [
        vec!["-P", "2222", "git@fixture"],
        vec!["-P2222", "git@fixture"],
        vec!["-vP2222", "git@fixture"],
        vec!["-batch", "git@fixture"],
        vec!["-ssh", "git@fixture"],
        vec!["--putty", "git@fixture"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(validate_ssh_options(&args).is_err_and(|e| e.contains("incompatible SSH options")));
    }
    for args in [
        vec![
            "-G",
            "-o",
            "SendEnv=GIT_PROTOCOL",
            "-p",
            "2222",
            "git@fixture",
            "git-upload-pack '/path'",
        ],
        vec![
            "-vv",
            "-4",
            "-F",
            "/fixture/-P-config",
            "-o",
            "ProxyCommand=tool -P 2222",
            "git@fixture",
        ],
        vec!["--", "git@fixture", "-P remote-command-data"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_ok()
        );
    }
}

#[test]
fn profile_openssh_checks_options_after_the_hostname_until_the_command_boundary() {
    for args in [
        vec!["git@fixture", "-P", "2222"],
        vec!["-vv", "git@fixture", "-vP2222"],
        vec!["git@fixture", "-batch"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
        );
    }
    for args in [
        vec!["git@fixture", "-p", "2222", "git-upload-pack '/x'"],
        vec!["--", "git@fixture", "-P2222"],
        vec!["git@fixture", "--", "-P2222"],
        vec!["git@fixture", "git-upload-pack /x", "-P2222"],
    ] {
        assert!(
            validate_ssh_options(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_ok()
        );
    }
}

#[test]
fn scoped_workers_keep_the_captured_profile_and_cancellation_even_after_context_changes()
-> Result<(), String> {
    let parent = crate::tasks::Cancellation::default();
    let a = Context {
        error: Some("captured profile".into()),
        ..Default::default()
    };
    let b = Context {
        error: Some("later profile".into()),
        ..Default::default()
    };
    crate::tasks::with_cancellation(parent.clone(), || {
        a.run(|| {
            let work = inherit(|| (current(), crate::tasks::cancellation().is_cancelled()));
            parent.cancel();
            b.run(|| {
                std::thread::scope(|scope| {
                    let (context, cancelled) = scope
                        .spawn(work)
                        .join()
                        .map_err(|_| "synthetic worker panicked".to_owned())?;
                    assert_eq!(context, a);
                    assert!(cancelled);
                    assert_eq!(current(), b);
                    Ok::<_, String>(())
                })
            })
        })
    })?;
    assert!(
        !crate::tasks::cancellation().is_cancelled(),
        "the caller's prior lifetime is restored"
    );
    Ok(())
}
