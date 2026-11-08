//! Shared explicit-account policy for ordinary, watched and custom Git runners.
use super::*;
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
) -> Result<CredentialCommandPolicy, String> {
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
        }
    }
    config.push(CREDENTIAL_HELPER.into());
    let arguments = config.into_iter().flat_map(|c| ["-c".into(), c]).collect();
    Ok(CredentialCommandPolicy {
        arguments,
        environment,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(configs.last().copied(), Some(CREDENTIAL_HELPER));
        assert!(!p.arguments.join(" ").contains("DUMMY_ALICE"));
        Ok(())
    }
}
