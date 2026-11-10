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
        TRANSPORT_ALIAS.contains("\"$GITCRAFT_GIT_ORIGINAL\" -C \"$launch_dir\" \"$@\"; result=$?")
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
        || parts
            .iter()
            .zip([4, 4, 8])
            .any(|(p, max)| p.is_empty() || p.len() > max || !p.bytes().all(|b| b.is_ascii_digit()))
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
    let check = TRANSPORT_ALIAS.find("version=$(\"$GITCRAFT_GIT_ORIGINAL\" --version) || return 1");
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
fn proactive_auth_is_https_only_exact_url_and_preserves_owned_configuration() -> Result<(), String>
{
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
fn policy_resets_wrong_helpers_and_overrides_specific_redirects_and_accounts() -> Result<(), String>
{
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
        .as_chunks::<2>()
        .0
        .iter()
        .map(|a| {
            assert_eq!(a[0], "-c");
            a[1].as_str()
        })
        .collect::<Vec<_>>();
    assert!(configs.contains(&"credential.helper="));
    assert!(configs.contains(&"credential.https://host.invalid/team/repo.helper="));
    assert!(
        configs.contains(&"http.https://host.invalid/team/repo/info/refs.followredirects=false")
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
