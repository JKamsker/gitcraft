//! Selected-repository endpoints and constrained repair/practice runners.

use super::*;

pub(crate) fn local_repair_input(
    dir: &Path,
    args: &[&str],
    input: &[u8],
    env: &[(&'static str, String)],
) -> Result<std::process::Output, String> {
    if remote::is_remote(dir) {
        return Err(
            crate::i18n::text("backend-local-repair-expected-a-local-repository-4833dc").into(),
        );
    }
    let mut command = local_command(Some(dir), &Extra::default(), &[]);
    for name in PREFLIGHT_REMOVE {
        command.env_remove(name);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .arg("--no-lazy-fetch")
        .args(args);
    crate::processes::run(
        command,
        crate::processes::Options {
            input: Some(input.to_vec()),
            timeout: Some(std::time::Duration::from_secs(15)),
            output_limit: 1024 * 1024,
            ..Default::default()
        },
    )?
    .complete()
}

pub(crate) fn repair_host(
    dir: &Path,
    script: &str,
    input: Vec<u8>,
    env: &[(&'static str, String)],
) -> Result<remote::Output, String> {
    let target = remote::parse(dir).ok_or(crate::i18n::text(
        "backend-expected-remote-repository-987cc6",
    ))?;
    let mut request = remote_request(&target, &Extra::default(), &[]);
    request.script = script.into();
    request.stdin = Some(input);
    request.long = true;
    request.timeout = Some(std::time::Duration::from_secs(15));
    request.env.push(("GIT_NO_LAZY_FETCH".into(), "1".into()));
    request
        .env
        .push(("GIT_NO_REPLACE_OBJECTS".into(), "1".into()));
    request
        .env
        .extend(env.iter().map(|(k, v)| ((*k).into(), v.clone())));
    request
        .unset
        .extend(PREFLIGHT_REMOVE.iter().map(|s| (*s).into()));
    remote::exec(&target.host, &request)
}

/// Bounded, read-only repository tour requests. Callers supply only fixed Git operations.
pub(crate) fn tour_read(
    repo: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    bounded::run(repo, args, input.unwrap_or_default().to_vec(), 64 << 20)
}

/// All local execution routes, including streaming and watched operations, use this builder.
pub(in crate::git) fn repository_command(dir: Option<&Path>, args: &[&str]) -> Command {
    if let Some(dir) = dir.filter(|dir| crate::practice::is_repository(dir)) {
        let mut cmd = practice_command(dir);
        object_read_policy(&mut cmd, args);
        return cmd;
    }
    let mut cmd = git_cmd();
    object_read_policy(&mut cmd, args);
    with_profile(&mut cmd, args);
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    cmd
}

pub(super) fn practice_command(dir: &Path) -> Command {
    let mut cmd = git_cmd();
    for name in crate::practice::ISOLATED_ENV_REMOVE {
        cmd.env_remove(name);
    }
    for (name, value) in crate::practice::ISOLATED_ENV_SET {
        cmd.env(name, value);
    }
    cmd.arg("--no-replace-objects").arg("-C").arg(dir);
    cmd
}

pub(crate) fn practice_run(dir: &Path, args: &[&str]) -> Result<String, String> {
    if remote::is_remote(dir) {
        return Err(crate::i18n::text("backend-practice-repositories-are-local-7d0527").into());
    }
    let mut cmd = practice_command(dir);
    cmd.args(args);
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        },
    )?
    .complete()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

pub(crate) fn onboarding_local_config(
    repo: &Path,
    key: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<String>, String> {
    if !["user.name", "user.email"].contains(&key) {
        return Err(crate::i18n::text("backend-unsupported-setup-configuration-key-c326ad").into());
    }
    if !active() {
        return Err("Export cancelled".into());
    }
    let bytes = execute(
        Some(repo),
        &Extra {
            timeout: Some(std::time::Duration::from_secs(20)),
            output_limit: Some(65536),
            ..Default::default()
        },
        &["config", "--local", "--null", "--get", key],
    )?;
    if bytes.code == Some(1) {
        return Ok(None);
    }
    if bytes.code != Some(0) {
        return Err(crate::i18n::text(
            "backend-could-not-inspect-local-setup-configuration-3f3361",
        )
        .into());
    }
    let value = bytes.stdout.strip_suffix(&[0]).ok_or(crate::i18n::text(
        "backend-invalid-configuration-response-260489",
    ))?;
    String::from_utf8(value.to_vec())
        .map(Some)
        .map_err(|_| crate::i18n::text("backend-local-configuration-is-not-utf-8-311338").into())
}

/// Pin an already reviewed effective endpoint using a private remote. Git expands
/// the private placeholder once; explicit pushurl disables pushInsteadOf. The
/// effective address is never handed back as an unqualified remote/URL operand.
pub(super) fn repository_endpoint_extra(
    repo: &Path,
    address: &str,
    original_remote: &str,
    credentials: Option<&Credentials>,
    original_args: &[&str],
) -> Result<(Extra, String), String> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let alias = format!("gitcraft-endpoint-{}-{id}", std::process::id());
    let placeholder = format!("gitcraft-pinned-address://{alias}");
    let mut values = vec![
        (format!("url.{address}.insteadOf"), placeholder.clone()),
        (format!("remote.{alias}.url"), String::new()),
        (format!("remote.{alias}.url"), placeholder.clone()),
        (format!("remote.{alias}.pushurl"), String::new()),
        (format!("remote.{alias}.pushurl"), placeholder.clone()),
    ];
    if !original_remote.is_empty() {
        values.extend([
            (format!("remote.{original_remote}.url"), String::new()),
            (format!("remote.{original_remote}.url"), placeholder.clone()),
            (format!("remote.{original_remote}.pushurl"), String::new()),
            (format!("remote.{original_remote}.pushurl"), placeholder),
        ]);
    }
    const VARS: [(&str, &str); 9] = [
        ("GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0"),
        ("GIT_CONFIG_KEY_1", "GIT_CONFIG_VALUE_1"),
        ("GIT_CONFIG_KEY_2", "GIT_CONFIG_VALUE_2"),
        ("GIT_CONFIG_KEY_3", "GIT_CONFIG_VALUE_3"),
        ("GIT_CONFIG_KEY_4", "GIT_CONFIG_VALUE_4"),
        ("GIT_CONFIG_KEY_5", "GIT_CONFIG_VALUE_5"),
        ("GIT_CONFIG_KEY_6", "GIT_CONFIG_VALUE_6"),
        ("GIT_CONFIG_KEY_7", "GIT_CONFIG_VALUE_7"),
        ("GIT_CONFIG_KEY_8", "GIT_CONFIG_VALUE_8"),
    ];
    let mut extra = Extra {
        normalize_repository: true,
        timeout: Some(std::time::Duration::from_secs(120)),
        output_limit: Some(1 << 20),
        ..Default::default()
    };
    extra
        .env
        .push(("GIT_CONFIG_COUNT", values.len().to_string()));
    for ((key, value), (k, v)) in values.into_iter().zip(VARS) {
        extra.env.extend([(k, key), (v, value)]);
    }
    if let Some(credentials) = credentials {
        // Validate legacy/edited state even when this destination has another account.
        credentials.helper_environment()?;
        if credentials
            .scope
            .as_ref()
            .is_some_and(|s| s.matches_url(address))
        {
            let policy = credential_command_policy(Some(repo), credentials, original_args)?;
            extra.owned_pre.extend(policy.arguments);
            extra.env.extend(policy.environment);
        }
    }
    Ok((extra, alias))
}

pub(in crate::git) fn repository_endpoint(
    repo: &Path,
    address: &str,
    original_remote: &str,
    before: &[&str],
    after: &[&str],
    credentials: Option<&Credentials>,
    control: &std::sync::Arc<OpControl>,
) -> Result<String, String> {
    let (extra, alias) =
        repository_endpoint_extra(repo, address, original_remote, credentials, before)?;
    let words: Vec<&str> = before
        .iter()
        .copied()
        .chain(std::iter::once(alias.as_str()))
        .chain(after.iter().copied())
        .collect();
    let (result, record) =
        watched::run_watched_extra_recorded(Some(repo), &words, credentials, false, control, extra);
    control.record(record);
    result
}
