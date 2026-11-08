//! Shared explicit-account policy for ordinary, watched and custom Git runners.
use super::*;
use crate::git::CREDENTIAL_HELPER;
#[derive(Clone, Debug)]
pub(crate) struct CredentialCommandPolicy {
    pub arguments: Vec<String>,
    pub environment: Vec<(&'static str, String)>,
}
impl CredentialCommandPolicy {
    /// Custom local runners bind the executable they actually selected, before
    /// installing this environment. Shared Git runners perform this automatically.
    #[expect(
        dead_code,
        reason = "Shared policy contract for custom local runners; current consumers use the shared builder's selected-executable pin"
    )]
    pub fn bind_git(&mut self, selected: &Path) -> Result<(), String> {
        if !selected.is_absolute() {
            return Err(crate::i18n::text(
                "backend-explicit-credentials-require-the-actual-selected-absolute-git-exe-41caa2",
            )
            .into());
        }
        self.environment
            .retain(|(key, _)| *key != "GITCRAFT_GIT_ORIGINAL");
        self.environment.push((
            "GITCRAFT_GIT_ORIGINAL",
            selected.to_string_lossy().into_owned(),
        ));
        Ok(())
    }
}
/// Explicit credentials are authoritative and transient. Ambient helpers are not
/// consulted or used for automatic post-success storage; persistence is a separate
/// user action in their credential manager. The HTTP wrapper installs exact
/// actual-URL policy in the transport context, after repository configuration.
pub(crate) fn credential_command_policy(
    dir: Option<&Path>,
    credentials: &Credentials,
    args: &[&str],
) -> Result<CredentialCommandPolicy, String> {
    if !cfg!(unix) && !dir.is_some_and(remote::is_remote) {
        return Err(crate::i18n::text(
            "backend-explicit-transient-https-sign-in-is-unavailable-for-local-windows-7d90ff",
        )
        .into());
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
                    crate::i18n::text("backend-this-command-attempts-to-replace-the-explicit-credential-transpor-082086")
                        .into(),
                );
            }
        }
    }
    let environment = credentials.helper_environment()?;
    if !credentials
        .scope
        .as_ref()
        .is_some_and(|scope| scope.is_https())
    {
        return Err(crate::i18n::text(
            "backend-explicit-transient-sign-in-requires-https-and-git-2-46-or-newer-o-1f40fa",
        )
        .into());
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
            return Err(crate::i18n::text(
                "backend-invalid-credential-transport-configuration-key-a7103f",
            )
            .into());
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
                    CREDENTIAL_HELPER.strip_prefix("credential.helper=").ok_or(
                        crate::i18n::text(
                            "backend-invalid-built-in-credential-helper-definition-e77905"
                        )
                    )?
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
    "test \"$#\" -ge 2 || exit 126\nurl=$2\ncase \"$url\" in https://*) ;; *) printf '%s\\n' 'Explicit transient sign-in requires HTTPS' >&2; exit 126;; esac\n",
    "authority=${url#*://}; authority=${authority%%/*}\ncase \"$authority\" in *@*) printf '%s\\n' 'Use a credential-free remote URL with explicit sign-in' >&2; exit 126;; esac\n",
    // remote-curl::end_url_with_slash feeds precisely this URL to http_init.
    // An exact actual-URL key is maximally specific even for new includeIf/config.
    "case \"$url\" in */) ;; *) url=$url/;; esac\n",
    "helper=${GITCRAFT_HTTP_HELPER#credential.helper=}\nexport GITCRAFT_PINNED_HTTP\n",
    "exec \"$GITCRAFT_GIT_ORIGINAL\" -c \"http.$url.followRedirects=false\" -c \"http.$url.proactiveAuth=basic\" -c \"http.$url.emptyAuth=false\" -c \"credential.$url.helper=\" -c \"credential.$url.helper=$helper\" -c \"credential.$url.username=$GITCRAFT_GIT_USERNAME\" -c \"credential.$url.useHttpPath=true\" -c \"http.$url.extraHeader=\" -c \"$GITCRAFT_HTTP_NATIVE_ALIAS\" gitcraft-http-native \"$@\"\n"
);
const TRANSPORT_ALIAS: &str = concat!(
    "alias.gitcraft-credential-transport=!f() { ",
    "unset GITCRAFT_CREDENTIAL_OWNER; ",
    "if test -n \"$GIT_PREFIX\"; then cd \"$GIT_PREFIX\" || return 126; fi; ",
    "unset GIT_EXEC_PATH; case \"$GITCRAFT_GIT_ORIGINAL\" in /*) ;; *) printf '%s\\n' 'Selected Git executable was not pinned' >&2; return 126;; esac; export GITCRAFT_GIT_ORIGINAL; ",
    // Git 2.46 first supplies the supported proactive Basic configuration.
    // Run only the pinned repository-machine executable; never fall back on an
    // unknown option being silently ignored. Version text is never echoed.
    "gitcraft_proactive_version() { ",
    "version=$(\"$GITCRAFT_GIT_ORIGINAL\" --version) || return 1; test \"${#version}\" -le 128 || return 1; ",
    "case \"$version\" in 'git version '*) version=${version#git version };; *) return 1;; esac; ",
    "case \"$version\" in *' (Apple Git-'*')') vendor=${version#* (Apple Git-}; vendor=${vendor%)}; case \"$vendor\" in ''|*[!0-9.]*|.*|*.|*..*) return 1;; esac; version=${version%% *};; *' '*) return 1;; esac; ",
    // Recognize the ordinary upstream numeric and Apple forms. Other vendor,
    // prerelease or native/MSYS forms require separate support, not a guess.
    "major=${version%%.*}; rest=${version#*.}; test \"$rest\" != \"$version\" || return 1; minor=${rest%%.*}; patch=${rest#*.}; test \"$patch\" != \"$rest\" || return 1; ",
    "for part in \"$major\" \"$minor\" \"$patch\"; do case \"$part\" in ''|*[!0-9]*) return 1;; esac; done; ",
    "test \"${#major}\" -le 4 && test \"${#minor}\" -le 4 && test \"${#patch}\" -le 8 || return 1; ",
    "test \"$major\" -gt 2 || { test \"$major\" -eq 2 && test \"$minor\" -ge 46; }; }; ",
    "gitcraft_proactive_version || { printf '%s\\n' 'Explicit transient HTTPS sign-in requires recognized Git 2.46 or newer on the repository machine; use the profile credential manager otherwise.' >&2; return 126; }; ",
    "original=$(\"$GITCRAFT_GIT_ORIGINAL\" --exec-path) || return 126; ",
    "case \"$original\" in /*) ;; *) printf '%s\\n' 'Credential transport needs an absolute original Git exec-path' >&2; return 126;; esac; ",
    "GITCRAFT_HTTP_ORIGINAL=$original/git-remote-http; GITCRAFT_HTTPS_ORIGINAL=$original/git-remote-https; ",
    "test -x \"$GITCRAFT_HTTP_ORIGINAL\" && test -x \"$GITCRAFT_HTTPS_ORIGINAL\" || return 126; ",
    "export GITCRAFT_HTTP_ORIGINAL GITCRAFT_HTTPS_ORIGINAL; ",
    "launch_dir=$PWD; umask 077; owned=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-credential-XXXXXXXX\") || return 126; cd \"$owned\" || return 126; owned=$PWD; ",
    // CWD retains the exclusive directory identity even if its name is replaced.
    // Retire only the fixed private files relative to that identity. Deliberately
    // retain the empty 0700 directory: POSIX shell cannot atomically prove the
    // identity of a pathname passed to rmdir after a separate identity check.
    "cleanup() { rm -f -- ./git-remote-http ./git-remote-https; }; ",
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
        let command = apply_command_extras(
            Command::new("/recording/selected/bin/git"),
            Some(Path::new("/nonexistent/recording")),
            &extra,
            &original,
        );
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
        for word in TRANSPORT_ALIAS.split(|c: char| c.is_whitespace() || c == ';') {
            assert!(!["ps", "pgrep", "pkill", "kill"].contains(&word));
        }
        assert!(!TRANSPORT_ALIAS.contains("rm -rf"));
        // Static contracts only; source/recording review must separately bind
        // the native HTTP/helper process chain. This is not a runtime PID proof.
        assert!(CREDENTIAL_HELPER.contains("test -z \"$GITCRAFT_CREDENTIAL_OWNER\""));
        assert!(CREDENTIAL_HELPER.contains("test \"$PPID\" != \"$GITCRAFT_CREDENTIAL_OWNER\""));
        assert!(CREDENTIAL_HELPER.contains("quit=true"));
    }
    // Inert interpretation of the emitted guard grammar. It consumes supplied
    // output/status, never a discovered executable; generated shell execution
    // remains a separately held native gate.
    fn recorded_version_supported(output: Result<&str, ()>) -> bool {
        let Ok(output) = output else {
            return false;
        };
        if output.len() > 128 {
            return false;
        }
        let Some(mut version) = output.strip_prefix("git version ") else {
            return false;
        };
        if let Some((number, vendor)) = version.split_once(" (Apple Git-") {
            let Some(vendor) = vendor.strip_suffix(')') else {
                return false;
            };
            if !vendor
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            {
                return false;
            }
            version = number;
        }
        let parts = version.split('.').collect::<Vec<_>>();
        if parts.len() != 3
            || parts.iter().zip([4, 4, 8]).any(|(p, max)| {
                p.is_empty() || p.len() > max || !p.bytes().all(|b| b.is_ascii_digit())
            })
        {
            return false;
        }
        let major = parts[0].parse::<u32>().ok();
        let minor = parts[1].parse::<u32>().ok();
        major
            .zip(minor)
            .is_some_and(|(major, minor)| major > 2 || (major == 2 && minor >= 46))
    }
    #[test]
    fn proactive_auth_guard_refuses_old_unknown_or_failed_pinned_versions() {
        for (version, supported) in [
            ("git version 2.45.9", false),
            ("git version 2.46.0", true),
            ("git version 2.47.1", true),
            ("git version 2.54.0", true),
            ("git version 2.39.5 (Apple Git-154)", false),
            ("git version 2.50.1 (Apple Git-155.1)", true),
            ("git version 2.46", false),
            ("git version 2.46.0-rc1", false),
            ("git version 2.46.0.windows.1", false),
            ("git version 2.46.0 (Unknown Git-1)", false),
            ("git version 2.46.0 (Apple Git-..)", false),
            ("git version 2.46.0\nsecret", false),
            ("unrecognized", false),
        ] {
            assert_eq!(
                recorded_version_supported(Ok(version)),
                supported,
                "{version}"
            );
        }
        assert!(!recorded_version_supported(Err(())));
        let pin = TRANSPORT_ALIAS.find("Selected Git executable was not pinned");
        let check =
            TRANSPORT_ALIAS.find("version=$(\"$GITCRAFT_GIT_ORIGINAL\" --version) || return 1");
        let discovery = TRANSPORT_ALIAS.find("--exec-path");
        let artifacts = TRANSPORT_ALIAS.find("mktemp -d");
        assert!(pin.zip(check).is_some_and(|(p, c)| p < c));
        assert!(check.zip(discovery).is_some_and(|(c, d)| c < d));
        assert!(discovery.zip(artifacts).is_some_and(|(d, a)| d < a));
        assert!(TRANSPORT_ALIAS.contains("gitcraft_proactive_version || { printf"));
        assert!(TRANSPORT_ALIAS.contains("test \"$minor\" -ge 46"));
        assert!(!TRANSPORT_ALIAS.contains("git --version"));
    }
    #[cfg(unix)]
    #[test]
    fn proactive_auth_is_https_only_exact_url_and_preserves_owned_configuration()
    -> Result<(), String> {
        let credentials = Credentials::for_url(
            "https://push.invalid/repo",
            "alice".into(),
            "DUMMY_SELECTED".into(),
        )?;
        let original = ["push", "private-alias", "HEAD:refs/heads/main"];
        let policy = credential_command_policy(
            Some(Path::new("/nonexistent/recorded")),
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
            ("HOME", "/recorded/profile-home".into()),
            ("GIT_SSL_CAINFO", "/recorded/profile-ca.pem".into()),
            ("GIT_CONFIG_COUNT", "2".into()),
            (
                "GIT_CONFIG_KEY_0",
                "http.https://push.invalid/repo/.proactiveAuth".into(),
            ),
            ("GIT_CONFIG_VALUE_0", "none".into()),
            (
                "GIT_CONFIG_KEY_1",
                "http.https://push.invalid/repo/.emptyAuth".into(),
            ),
            ("GIT_CONFIG_VALUE_1", "true".into()),
        ]);
        let command = apply_command_extras(
            Command::new("/recorded/repository-machine/git"),
            Some(Path::new("/nonexistent/recorded")),
            &extra,
            &original,
        );
        let env = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            env.get("HOME"),
            Some(&Some("/recorded/profile-home".into()))
        );
        assert_eq!(
            env.get("GIT_SSL_CAINFO"),
            Some(&Some("/recorded/profile-ca.pem".into()))
        );
        assert_eq!(env.get("GIT_CONFIG_VALUE_0"), Some(&Some("none".into())));
        assert_eq!(env.get("GIT_CONFIG_VALUE_1"), Some(&Some("true".into())));
        let wrapper = env
            .get("GITCRAFT_HTTP_WRAPPER")
            .and_then(|v| v.as_deref())
            .ok_or("Missing wrapper")?;
        assert!(wrapper.contains("http.$url.proactiveAuth=basic"));
        assert!(wrapper.contains("http.$url.emptyAuth=false"));
        assert!(wrapper.contains("case \"$url\" in https://*)"));
        assert!(wrapper.contains("credential.$url.username=$GITCRAFT_GIT_USERNAME"));
        assert!(wrapper.contains("credential.$url.helper="));
        assert!(!wrapper.contains("DUMMY_SELECTED"));
        assert!(
            !command
                .get_args()
                .any(|a| a.to_string_lossy().contains("DUMMY_SELECTED"))
        );
        assert_eq!(
            env.get("GITCRAFT_GIT_ORIGINAL"),
            Some(&Some("/recorded/repository-machine/git".into()))
        );
        let plain = Credentials::for_url(
            "http://push.invalid/repo",
            "alice".into(),
            "DUMMY_SELECTED".into(),
        )?;
        assert!(
            credential_command_policy(Some(Path::new("/nonexistent/recorded")), &plain, &original)
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn retirement_uses_the_retained_directory_and_leaves_its_empty_entry() {
        let entry = TRANSPORT_ALIAS.find("cd \"$owned\" || return 126");
        let cleanup =
            TRANSPORT_ALIAS.find("cleanup() { rm -f -- ./git-remote-http ./git-remote-https; };");
        assert!(
            entry
                .zip(cleanup)
                .is_some_and(|(entry, cleanup)| entry < cleanup)
        );
        assert!(TRANSPORT_ALIAS.contains("trap cleanup EXIT"));
        assert!(!TRANSPORT_ALIAS.contains("rmdir"));
        assert!(!TRANSPORT_ALIAS.contains("-ef"));
        assert!(!TRANSPORT_ALIAS.contains("rm -f -- \"$owned"));
        // Inert source/recording contract: no execution of cleanup, renames, or children.
        // The empty private directory is intentionally retained, including after failure.
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
