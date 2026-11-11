//! CLI account trust, bounded credential probes, and output redaction.
use super::*;

pub(super) fn trusted_host(forge: Forge, tool: &str, host: &str) -> Result<bool, String> {
    let context = crate::connections::current();
    let account = if tool == "gh" {
        &context.choices.github
    } else {
        &context.choices.gitlab
    };
    if account.selected() {
        account.validate()?;
        return Ok(account.host.eq_ignore_ascii_case(host));
    }

    use std::sync::{Mutex, OnceLock};
    type Known = Mutex<HashMap<(Forge, String), (bool, Instant)>>;
    static KNOWN: OnceLock<Known> = OnceLock::new();
    let plain = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        && !host.starts_with(['-', '.']);
    if !plain {
        return Ok(false);
    }
    let own = match forge {
        Forge::GitLab => "gitlab.com",
        _ => "github.com",
    };
    if host.eq_ignore_ascii_case(own) {
        return Ok(true);
    }
    let known = KNOWN.get_or_init(Known::default);
    let lock = || {
        known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let key = (forge, host.to_lowercase());
    if let Some(&(signed_in, at)) = lock().get(&key)
        && (signed_in || at.elapsed() < NOT_SIGNED_IN_FOR)
    {
        return Ok(signed_in);
    }
    let signed_in = signed_in_to(tool, host)?;
    lock().insert(key, (signed_in, Instant::now()));
    Ok(signed_in)
}

/// Whether `tool` reports a login within [`AUTH_CHECK_FOR`]. An incomplete check stays an
/// error, so cancellation, timeout or uncertain cleanup cannot be cached as signed out.
pub(super) fn signed_in_to(tool: &str, host: &str) -> Result<bool, String> {
    let mut cmd = tool_command(tool);
    for key in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
        "GITLAB_TOKEN",
        "GITLAB_ACCESS_TOKEN",
        "OAUTH_TOKEN",
        "CI_JOB_TOKEN",
    ] {
        cmd.env_remove(key);
    }
    cmd.args(["auth", "status", "--hostname", host])
        .current_dir(std::env::temp_dir());
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, None) {
        return Ok(answer.is_ok());
    }
    let options = auth_options(AUTH_CHECK_FOR, false)?;
    // No auth output reaches an error message, including partial output on failure.
    finish_auth(crate::processes::run_checked(cmd, options)).map_err(|error| {
        crate::i18n::message(
            "backend-couldn-t-check-tool-login-error-2f36d0",
            &[
                ("tool", (tool).to_string().into()),
                ("detail", (error).to_string().into()),
            ],
        )
    })
}

pub(super) fn finish_auth(
    result: Result<crate::processes::Output, crate::processes::Failure>,
) -> Result<bool, String> {
    result
        .map_err(crate::processes::Failure::message)?
        .complete()
        .map(|out| out.status.success())
}

#[cfg(windows)]
pub(super) const NULL_SINK: &str = "NUL";
#[cfg(not(windows))]
pub(super) const NULL_SINK: &str = "/dev/null";

pub(super) fn auth_options(
    within: Duration,
    keep: bool,
) -> Result<crate::processes::Options, String> {
    auth_options_with(within, keep, |path| {
        std::fs::OpenOptions::new().write(true).open(path)
    })
}

pub(super) fn auth_options_with(
    within: Duration,
    keep: bool,
    open_sink: impl FnOnce(&str) -> std::io::Result<std::fs::File>,
) -> Result<crate::processes::Options, String> {
    let mut options = bounded_options(within, None);
    options.output_limit = ERROR_LIMIT;
    // Preserve byte accounting so excessive stderr still stops the command, but never retain
    // tokens or account details in the captured result (including diagnostics on failure).
    options.stderr_filter = Some(Box::new(|bytes| vec![0; bytes.len()]));
    if !keep {
        // Service overrides Command.stdout: redirect through its file option, so `gh auth
        // token` goes directly to the OS null device and is never captured in memory.
        options.stdout_file = Some(open_sink(NULL_SINK).map_err(|error| {
            crate::i18n::message(
                "backend-couldn-t-open-the-null-output-sink-error-65c977",
                &[("detail", (error).to_string().into())],
            )
        })?);
    }
    Ok(options)
}

/// How long the tool may take to say whether it is signed in to a host.
pub(super) const AUTH_CHECK_FOR: Duration = Duration::from_secs(10);
/// How long a host the tool isn't signed in to is taken as such before it is asked again (the
/// user may sign in meanwhile).
pub(super) const NOT_SIGNED_IN_FOR: Duration = Duration::from_secs(60);
