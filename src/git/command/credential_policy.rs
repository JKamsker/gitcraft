//! Shared explicit-account policy for ordinary, watched and custom Git runners.
use super::*;
use crate::git::CREDENTIAL_HELPER;
#[derive(Clone, Debug)]
pub(crate) struct CredentialCommandPolicy {
    pub arguments: Vec<String>,
    pub environment: Vec<(&'static str, String)>,
}
/// Explicit credentials are authoritative and transient. Ambient helpers are not
/// consulted or used for automatic post-success storage; persistence is a separate
/// user action in their credential manager. Enumerate URL-specific keys too, as
/// their higher URL-match specificity can override a generic command-line value.
pub(crate) fn credential_command_policy(
    dir: Option<&Path>,
    credentials: &Credentials,
    args: &[&str],
) -> Result<CredentialCommandPolicy, String> {
    if !cfg!(unix) && !dir.is_some_and(remote::is_remote) {
        return Err("Explicit transient HTTPS sign-in is unavailable for local Windows Git: the native/MSYS transport identity boundary has not been established. Use an explicitly selected profile credential manager instead.".into());
    }
    for pair in args.windows(2) {
        if pair[0] == "-c" {
            let key = pair[1]
                .split('=')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if key.starts_with("credential.")
                || (key.starts_with("http.") && key.ends_with(".followredirects"))
                || key == "alias.gitcraft-credential-transport"
            {
                return Err(
                    "This command attempts to replace the explicit credential transport policy"
                        .into(),
                );
            }
        }
    }
    if let Some(dir) = dir {
        for url in operation_endpoints(dir, args)? {
            if let Some((_, rest)) = url.split_once("://")
                && let Some(authority) = rest.split('/').next()
                && let Some((userinfo, _)) = authority.rsplit_once('@')
            {
                // A URL-supplied password bypasses helper selection entirely.
                crate::git::CredentialScope::for_url(&url, &credentials.username)?;
                if userinfo.contains(':') {
                    return Err(
                        "Remove embedded remote credentials before using explicit sign-in".into(),
                    );
                }
            }
        }
    }
    let environment = credentials.helper_environment()?;
    let keys = connection_read(
        dir.unwrap_or_else(|| Path::new(".")),
        &[
            "config",
            "--null",
            "--name-only",
            "--get-regexp",
            "^(credential|http)\\.",
        ],
    )?;
    policy_for_keys(
        credentials,
        environment,
        keys.split('\0').filter(|k| !k.is_empty()),
    )
}
fn policy_for_keys<'a>(
    credentials: &Credentials,
    environment: Vec<(&'static str, String)>,
    keys: impl Iterator<Item = &'a str>,
) -> Result<CredentialCommandPolicy, String> {
    let mut config = vec![
        "credential.helper=".to_owned(),
        "credential.useHttpPath=true".into(),
        format!("credential.username={}", credentials.username),
        "http.followRedirects=false".into(),
    ];
    for key in keys {
        if key.len() > 32768 || key.chars().any(char::is_control) {
            return Err("Invalid credential transport configuration key".into());
        }
        let lower = key.to_ascii_lowercase();
        let value = if lower.starts_with("http.") && lower.ends_with(".followredirects") {
            Some("false")
        } else if lower.starts_with("credential.") && lower.ends_with(".helper") {
            Some("")
        } else if lower.starts_with("credential.") && lower.ends_with(".usehttppath") {
            Some("true")
        } else if lower.starts_with("credential.") && lower.ends_with(".username") {
            Some(credentials.username.as_str())
        } else {
            None
        };
        if let Some(value) = value {
            config.push(format!("{key}={value}"));
            if lower.starts_with("credential.") && lower.ends_with(".helper") {
                config.push(format!(
                    "{key}={}",
                    CREDENTIAL_HELPER
                        .strip_prefix("credential.helper=")
                        .expect("fixed helper")
                ));
            }
        }
    }
    config.push(CREDENTIAL_HELPER.into());
    config.push(TRANSPORT_ALIAS.into());
    let mut arguments: Vec<String> = config.into_iter().flat_map(|c| ["-c".into(), c]).collect();
    arguments.push("gitcraft-credential-transport".into());
    let mut environment = environment;
    environment.push(("GITCRAFT_HTTP_WRAPPER", TRANSPORT_WRAPPER.into()));
    Ok(CredentialCommandPolicy {
        arguments,
        environment,
    })
}
/// Executed on the repository machine by that machine's Git/sh. Only these
/// native HTTP executables can mint an owner. Standard Git waits for its HTTP
/// helpers before returning; the private artifact lifetime covers that wait.
/// Detached user programs deliberately reading/minting credentials are trusted
/// code, outside this consumer policy. No PID search or signal is involved.
const TRANSPORT_WRAPPER: &str = concat!(
    "#!/bin/sh\n",
    "case \"$0\" in */git-remote-http) transport=$GITCRAFT_HTTP_ORIGINAL;; */git-remote-https) transport=$GITCRAFT_HTTPS_ORIGINAL;; *) exit 126;; esac\n",
    "case \"$transport\" in /*) ;; *) exit 126;; esac\n",
    "GITCRAFT_CREDENTIAL_OWNER=$$; export GITCRAFT_CREDENTIAL_OWNER\n",
    "exec \"$transport\" \"$@\"\n"
);
const TRANSPORT_ALIAS: &str = concat!(
    "alias.gitcraft-credential-transport=!f() { ",
    "unset GITCRAFT_CREDENTIAL_OWNER; ",
    "if test -n \"$GIT_PREFIX\"; then cd \"$GIT_PREFIX\" || return 126; fi; ",
    "unset GIT_EXEC_PATH; original=$(git --exec-path) || return 126; ",
    "case \"$original\" in /*) ;; *) printf '%s\\n' 'Credential transport needs an absolute original Git exec-path' >&2; return 126;; esac; ",
    "GITCRAFT_HTTP_ORIGINAL=$original/git-remote-http; GITCRAFT_HTTPS_ORIGINAL=$original/git-remote-https; ",
    "test -x \"$GITCRAFT_HTTP_ORIGINAL\" && test -x \"$GITCRAFT_HTTPS_ORIGINAL\" || return 126; ",
    "export GITCRAFT_HTTP_ORIGINAL GITCRAFT_HTTPS_ORIGINAL; ",
    "umask 077; owned=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-credential-XXXXXXXX\") || return 126; ",
    "cleanup() { test -d \"$owned\" && test ! -L \"$owned\" || return; rm -f -- \"$owned/git-remote-http\" \"$owned/git-remote-https\"; rmdir -- \"$owned\"; }; ",
    "trap cleanup EXIT; ",
    "(set -C; printf '%s' \"$GITCRAFT_HTTP_WRAPPER\" > \"$owned/git-remote-http\" && printf '%s' \"$GITCRAFT_HTTP_WRAPPER\" > \"$owned/git-remote-https\") || return 126; ",
    "chmod 700 \"$owned/git-remote-http\" \"$owned/git-remote-https\" || return 126; ",
    "GIT_EXEC_PATH=$owned; PATH=$original:$PATH; export GIT_EXEC_PATH PATH; ",
    "git \"$@\"; result=$?; return \"$result\"; }; f"
);
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_artifacts_and_consumer_chain_are_explicit_without_process_searches() {
        assert!(TRANSPORT_ALIAS.contains("unset GITCRAFT_CREDENTIAL_OWNER"));
        assert!(TRANSPORT_ALIAS.contains("unset GIT_EXEC_PATH; original=$(git --exec-path)"));
        assert!(TRANSPORT_ALIAS.contains("mktemp -d"));
        assert!(TRANSPORT_ALIAS.contains("set -C"));
        assert!(TRANSPORT_ALIAS.contains("git \"$@\"; result=$?"));
        assert!(TRANSPORT_WRAPPER.contains("GITCRAFT_CREDENTIAL_OWNER=$$"));
        assert!(TRANSPORT_WRAPPER.contains("exec \"$transport\" \"$@\""));
        assert!(CREDENTIAL_HELPER.contains("$PPID"));
        for forbidden in ["ps ", "pgrep", "pkill", "kill ", "rm -rf"] {
            assert!(!TRANSPORT_ALIAS.contains(forbidden));
        }
        // Static contracts only; source/recording review must separately bind
        // the native HTTP/helper process chain. This is not a runtime PID proof.
        assert!(CREDENTIAL_HELPER.contains("test -z \"$GITCRAFT_CREDENTIAL_OWNER\""));
        assert!(CREDENTIAL_HELPER.contains("test \"$PPID\" != \"$GITCRAFT_CREDENTIAL_OWNER\""));
        assert!(CREDENTIAL_HELPER.contains("quit=true"));
    }
    #[test]
    fn policy_resets_wrong_helpers_and_overrides_specific_redirects_and_accounts()
    -> Result<(), String> {
        let c = Credentials::for_url(
            "https://host.invalid/team/repo/",
            "alice".into(),
            "DUMMY_ALICE".into(),
        )?;
        let p = policy_for_keys(
            &c,
            c.helper_environment()?,
            [
                "credential.helper",
                "credential.https://host.invalid.username",
                "credential.https://host.invalid/team/repo.helper",
                "http.https://host.invalid/team/repo/info/refs.followredirects",
                "credential.https://host.invalid.usehttppath",
            ]
            .into_iter(),
        )?;
        let configs = p
            .arguments
            .chunks_exact(2)
            .map(|a| {
                assert_eq!(a[0], "-c");
                a[1].as_str()
            })
            .collect::<Vec<_>>();
        assert!(configs.contains(&"credential.helper="));
        assert!(configs.contains(&"credential.https://host.invalid/team/repo.helper="));
        assert!(
            configs
                .contains(&"http.https://host.invalid/team/repo/info/refs.followredirects=false")
        );
        assert!(configs.contains(&"credential.https://host.invalid.username=alice"));
        assert!(configs.contains(&"credential.https://host.invalid.usehttppath=true"));
        assert!(configs.contains(&CREDENTIAL_HELPER));
        assert_eq!(
            p.arguments.last().map(String::as_str),
            Some("gitcraft-credential-transport")
        );
        assert!(!p.arguments.join(" ").contains("DUMMY_ALICE"));
        Ok(())
    }
}
