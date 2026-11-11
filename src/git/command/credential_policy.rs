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
    #[cfg(all(test, unix))]
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
mod tests;
