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
