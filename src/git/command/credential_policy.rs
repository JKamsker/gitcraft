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
    let mut environment = credentials.helper_environment()?;
    if !dir.is_some_and(remote::is_remote) {
        let selected = git_cmd().get_program().to_owned();
        if !Path::new(&selected).is_absolute() {
            return Err("Could not pin the selected Git executable for explicit sign-in".into());
        }
        environment.push((
            "GITCRAFT_GIT_ORIGINAL",
            selected.to_string_lossy().into_owned(),
        ));
    }
    // The owning runner may carry private, command-only remote configuration.
    // Do not resolve its alias or inspect a different config here. Actual URL
    // policy is installed by the designated transport at the point of launch.
    policy_for_keys(credentials, environment, std::iter::empty())
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
                        .ok_or("Invalid built-in credential helper definition")?
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
    environment.push(("GITCRAFT_HTTP_NATIVE_ALIAS", NATIVE_HTTP_ALIAS.into()));
    environment.push(("GITCRAFT_HTTP_HELPER", CREDENTIAL_HELPER.into()));
    Ok(CredentialCommandPolicy {
        arguments,
        environment,
    })
}
/// Prefix for custom remote runners using this policy. Resolve on that machine,
/// before launching the outer Git; never substitute this computer's executable.
pub(crate) const CREDENTIAL_REMOTE_PIN: &str = "GITCRAFT_GIT_ORIGINAL=$(command -v git) || exit 126\ncase \"$GITCRAFT_GIT_ORIGINAL\" in /*) ;; *) exit 126;; esac\nexport GITCRAFT_GIT_ORIGINAL\n";
/// Executed on the repository machine by that machine's Git/sh. Only these
/// native HTTP executables can mint an owner. Standard Git waits for its HTTP
/// helpers before returning; the private artifact lifetime covers that wait.
/// Detached user programs deliberately reading/minting credentials are trusted
/// code, outside this consumer policy. No PID search or signal is involved.
const NATIVE_HTTP_ALIAS: &str = "alias.gitcraft-http-native=!f() { GITCRAFT_CREDENTIAL_OWNER=$$; export GITCRAFT_CREDENTIAL_OWNER; exec \"$GITCRAFT_PINNED_HTTP\" \"$@\"; }; f";
const TRANSPORT_WRAPPER: &str = concat!(
    "#!/bin/sh\n",
    "case \"$0\" in */git-remote-http) GITCRAFT_PINNED_HTTP=$GITCRAFT_HTTP_ORIGINAL;; */git-remote-https) GITCRAFT_PINNED_HTTP=$GITCRAFT_HTTPS_ORIGINAL;; *) exit 126;; esac\n",
    "case \"$GITCRAFT_PINNED_HTTP\" in /*) ;; *) exit 126;; esac\n",
    "test \"$#\" -ge 2 || exit 126\nurl=$2\ncase \"$url\" in http://*|https://*) ;; *) exit 126;; esac\n",
    "authority=${url#*://}; authority=${authority%%/*}\ncase \"$authority\" in *@*) printf '%s\\n' 'Use a credential-free remote URL with explicit sign-in' >&2; exit 126;; esac\n",
    // remote-curl::end_url_with_slash feeds precisely this URL to http_init.
    // An exact actual-URL key is maximally specific even for new includeIf/config.
    "case \"$url\" in */) ;; *) url=$url/;; esac\n",
    "helper=${GITCRAFT_HTTP_HELPER#credential.helper=}\nexport GITCRAFT_PINNED_HTTP\n",
    "exec \"$GITCRAFT_GIT_ORIGINAL\" -c \"http.$url.followRedirects=false\" -c \"credential.$url.helper=\" -c \"credential.$url.helper=$helper\" -c \"credential.$url.username=$GITCRAFT_GIT_USERNAME\" -c \"credential.$url.useHttpPath=true\" -c \"http.$url.extraHeader=\" -c \"$GITCRAFT_HTTP_NATIVE_ALIAS\" gitcraft-http-native \"$@\"\n"
);
const TRANSPORT_ALIAS: &str = concat!(
    "alias.gitcraft-credential-transport=!f() { ",
    "unset GITCRAFT_CREDENTIAL_OWNER; ",
    "if test -n \"$GIT_PREFIX\"; then cd \"$GIT_PREFIX\" || return 126; fi; ",
    "unset GIT_EXEC_PATH; case \"$GITCRAFT_GIT_ORIGINAL\" in /*) ;; *) printf '%s\\n' 'Selected Git executable was not pinned' >&2; return 126;; esac; export GITCRAFT_GIT_ORIGINAL; ",
    "original=$(\"$GITCRAFT_GIT_ORIGINAL\" --exec-path) || return 126; ",
    "case \"$original\" in /*) ;; *) printf '%s\\n' 'Credential transport needs an absolute original Git exec-path' >&2; return 126;; esac; ",
    "GITCRAFT_HTTP_ORIGINAL=$original/git-remote-http; GITCRAFT_HTTPS_ORIGINAL=$original/git-remote-https; ",
    "test -x \"$GITCRAFT_HTTP_ORIGINAL\" && test -x \"$GITCRAFT_HTTPS_ORIGINAL\" || return 126; ",
    "export GITCRAFT_HTTP_ORIGINAL GITCRAFT_HTTPS_ORIGINAL; ",
    "launch_dir=$PWD; umask 077; owned=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-credential-XXXXXXXX\") || return 126; cd \"$owned\" || return 126; owned=$PWD; ",
    "cleanup() { rm -f -- ./git-remote-http ./git-remote-https; if test \"$owned\" -ef .; then rmdir -- \"$owned\"; fi; }; ",
    "trap cleanup EXIT; ",
    "(set -C; printf '%s' \"$GITCRAFT_HTTP_WRAPPER\" > ./git-remote-http && printf '%s' \"$GITCRAFT_HTTP_WRAPPER\" > ./git-remote-https) || return 126; ",
    "chmod 700 ./git-remote-http ./git-remote-https || return 126; ",
    "GIT_EXEC_PATH=$owned; PATH=$original:$PATH; export GIT_EXEC_PATH PATH; ",
    "\"$GITCRAFT_GIT_ORIGINAL\" -C \"$launch_dir\" \"$@\"; result=$?; return \"$result\"; }; f"
);
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn private_alias_configuration_is_preserved_by_the_production_builder_without_metadata_execution()
    -> Result<(), String> {
        let credentials = Credentials::for_url(
            "https://push.invalid/team/repo",
            "alice".into(),
            "DUMMY".into(),
        )?;
        let original = ["push", "reviewed-private-alias", "HEAD:refs/heads/main"];
        let policy = credential_command_policy(
            Some(Path::new("/nonexistent/recording")),
            &credentials,
            &original,
        )?;
        let mut extra = Extra {
            normalize_repository: true,
            owned_pre: policy.arguments,
            env: policy.environment,
            ..Default::default()
        };
        extra.env.extend([
            ("GIT_CONFIG_COUNT", "1".into()),
            (
                "GIT_CONFIG_KEY_0",
                "remote.reviewed-private-alias.url".into(),
            ),
            (
                "GIT_CONFIG_VALUE_0",
                "https://push.invalid/team/repo".into(),
            ),
        ]);
        let command = local_command(Some(Path::new("/nonexistent/recording")), &extra, &original);
        let args = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.ends_with(&original.map(String::from)));
        assert!(!args.join(" ").contains("DUMMY"));
        let env = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(env.get("GIT_CONFIG_COUNT"), Some(&Some("1".into())));
        assert_eq!(
            env.get("GIT_CONFIG_KEY_0"),
            Some(&Some("remote.reviewed-private-alias.url".into()))
        );
        assert_eq!(
            env.get("GIT_CONFIG_VALUE_0"),
            Some(&Some("https://push.invalid/team/repo".into()))
        );
        assert_eq!(env.get("GIT_DIR"), Some(&None));
        assert_eq!(
            env.get("GITCRAFT_GIT_ORIGINAL"),
            Some(&Some(command.get_program().to_string_lossy().into_owned()))
        );
        Ok(())
    }
    #[test]
    fn transport_artifacts_and_consumer_chain_are_explicit_without_process_searches() {
        assert!(TRANSPORT_ALIAS.contains("unset GITCRAFT_CREDENTIAL_OWNER"));
        assert!(TRANSPORT_ALIAS.contains("Selected Git executable was not pinned"));
        assert!(CREDENTIAL_REMOTE_PIN.contains("command -v git"));
        assert!(TRANSPORT_ALIAS.contains("mktemp -d"));
        assert!(TRANSPORT_ALIAS.contains("set -C"));
        assert!(
            TRANSPORT_ALIAS
                .contains("\"$GITCRAFT_GIT_ORIGINAL\" -C \"$launch_dir\" \"$@\"; result=$?")
        );
        assert!(NATIVE_HTTP_ALIAS.contains("GITCRAFT_CREDENTIAL_OWNER=$$"));
        assert!(NATIVE_HTTP_ALIAS.contains("exec \"$GITCRAFT_PINNED_HTTP\" \"$@\""));
        assert!(TRANSPORT_WRAPPER.contains("http.$url.followRedirects=false"));
        assert!(TRANSPORT_WRAPPER.contains("credential.$url.helper="));
        assert!(TRANSPORT_WRAPPER.contains("credential.$url.username=$GITCRAFT_GIT_USERNAME"));
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
