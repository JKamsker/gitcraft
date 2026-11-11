// Token fixtures. Included to preserve libtest shard names.
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
        let answer = context.run(|| {
            crate::forge::tool(
                &crate::forge::Hosting {
                    forge: crate::forge::Forge::GitHub,
                    host: "github.com".into(),
                    path: "team/app".into(),
                    web: "https://github.com".into(),
                },
                &repo,
                &["pr", "list"],
            )
        });
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
