//! Running a forge's own command-line tool (`gh`, `glab`), which holds the user's login.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::{Forge, Hosting};

/// Runs `tool` in `repo`; `noun` says what it's for when it isn't installed.
pub(super) fn run(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
) -> Result<String, String> {
    run_with_input(tool, noun, repo, hosting, args, None)
}

/// [`run`], with `input` on its stdin.
pub(super) fn run_with_input(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
    input: Option<&str>,
) -> Result<String, String> {
    if input.is_some_and(|text| text.len() > INPUT_LIMIT) {
        return Err(crate::i18n::message(
            "backend-tool-input-exceeds-input-limit-bytes-202e7a",
            &[
                ("tool", (tool).to_string().into()),
                ("input_limit", INPUT_LIMIT.into()),
            ],
        ));
    }
    let (cmd, start_error) = prepared(tool, noun, repo, hosting, args)?;
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, input) {
        return answer;
    }
    let options = bounded_options(TOOL_TIMEOUT, input);
    finish_run(
        tool,
        TOOL_TIMEOUT,
        dispatch(cmd, options, start_error, crate::processes::run_checked),
    )
}

/// [`run`], with the caller's deadline (including time queued in the process service).
pub(super) fn run_within(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
    within: Duration,
) -> Result<String, String> {
    let (cmd, start_error) = prepared(tool, noun, repo, hosting, args)?;
    within_command(tool, cmd, start_error, within)
}

fn within_command(
    tool: &str,
    cmd: Command,
    start_error: impl FnOnce(String) -> String,
    within: Duration,
) -> Result<String, String> {
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, None) {
        return answer;
    }
    finish_run(
        tool,
        within,
        dispatch(
            cmd,
            bounded_options(within, None),
            start_error,
            crate::processes::run_checked,
        ),
    )
}

// Network commands get two minutes; inbox callers supply their shorter deadline.
const TOOL_TIMEOUT: Duration = Duration::from_secs(120);
const INPUT_LIMIT: usize = 16 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const ERROR_LIMIT: usize = 64 * 1024;

fn bounded_options(within: Duration, input: Option<&str>) -> crate::processes::Options {
    crate::processes::Options {
        timeout: Some(within),
        // Service ignores BrokenPipe while still collecting the tool's exit diagnostics.
        input: input.map(|text| text.as_bytes().to_vec()),
        output_limit: OUTPUT_LIMIT,
        stderr_limit: Some(ERROR_LIMIT),
        ..Default::default()
    }
}

fn dispatch(
    command: Command,
    options: crate::processes::Options,
    start_error: impl FnOnce(String) -> String,
    run: impl FnOnce(
        Command,
        crate::processes::Options,
    ) -> Result<crate::processes::Output, crate::processes::Failure>,
) -> Result<crate::processes::Output, String> {
    run(command, options).map_err(|error| match error {
        crate::processes::Failure::Failed(message) => start_error(message),
        other => other.message(),
    })
}

fn finish_run(
    tool: &str,
    within: Duration,
    result: Result<crate::processes::Output, String>,
) -> Result<String, String> {
    let output = result?;
    // Only confirmed timeout cleanup gets the familiar deadline wording. Uncertain cleanup
    // retains its complete() error, even if the direct child exited successfully.
    let timed_out = output.stop_kind == Some(crate::processes::StopKind::TimedOut);
    let out = output.complete().map_err(|error| {
        if timed_out {
            crate::i18n::message(
                "backend-tool-didn-t-finish-within-seconds-error-f169ac",
                &[
                    ("tool", (tool).to_string().into()),
                    ("seconds", (within.as_secs()).into()),
                    ("detail", (error).to_string().into()),
                ],
            )
        } else {
            error
        }
    })?;
    answer(tool, &out)
}

/// What the tool printed, or why it failed.
fn answer(tool: &str, out: &std::process::Output) -> Result<String, String> {
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(failure(tool, out))
    }
}

/// The forge's tool, set to answer this repository in plain JSON.
fn tool_command(tool: &str) -> Command {
    let mut cmd = Command::new(crate::connections::program(tool));
    cmd.env("GH_PROMPT_DISABLED", "1").env("NO_PROMPT", "1");
    // What it prints is read as JSON: `CLICOLOR_FORCE` or `GH_FORCE_TTY` in the user's
    // environment would colour it even into a pipe, escape codes no JSON reader takes.
    cmd.env_remove("CLICOLOR_FORCE")
        .env_remove("GH_FORCE_TTY")
        .env("NO_COLOR", "1");
    // About the repository it runs in and the host it is given, not one the environment names:
    // `GH_REPO` would answer for another repository, `GH_HOST` send `gh api` (and `-R owner/repo`)
    // to another host than github.com, `GITLAB_HOST` point `glab` at another GitLab than the
    // remote's.
    cmd.env_remove("GH_REPO")
        .env_remove("GH_HOST")
        .env_remove("GITLAB_HOST")
        // glab's own names for the same: the repository, the host, a fork's head project.
        .env_remove("GITLAB_REPO")
        .env_remove("GITLAB_URI")
        .env_remove("GL_HOST")
        .env_remove("GITLAB_HEAD_REPO")
        .env_remove("GITLAB_API_HOST");
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// What a tool that failed said, or that it failed: an empty error would read as nothing wrong.
fn failure(tool: &str, out: &std::process::Output) -> String {
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
    let stderr = said(&out.stderr);
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = said(&out.stdout);
    if !stdout.is_empty() {
        return stdout;
    }
    crate::i18n::message(
        "backend-tool-failed-4a996a",
        &[
            ("tool", (tool).to_string().into()),
            ("status", (out.status).to_string().into()),
        ],
    )
}

/// What diagnostics say about a forge's tool: its version, and whether it has a login (never
/// any token or account).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolReport {
    pub tool: &'static str,
    /// The first line of `tool --version`; `None` when it isn't installed or didn't answer.
    pub version: Option<String>,
    /// Whether it has a login: `None` when that couldn't be told.
    pub signed_in: Option<bool>,
}

/// How long a tool may take to answer diagnostics.
const REPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// [`ToolReport`] for `gh` (`auth token` exits 0 when a login is stored, without asking the
/// server; the token itself is never read), `glab` (`auth status`, which asks the server) or
/// `tea` (its configured logins counted).
pub(crate) fn tool_report(tool: &'static str) -> ToolReport {
    let version = report_answer(tool, &["--version"], true).and_then(|(ok, out)| {
        ok.then(|| out.lines().next().unwrap_or_default().trim().to_owned())
            .filter(|v| !v.is_empty())
    });
    let signed_in = match tool {
        _ if version.is_none() => None,
        "tea" => Some(!super::gitea::tea_servers().is_empty()),
        "gh" => report_answer(tool, &["auth", "token"], false).map(|(ok, _)| ok),
        _ => report_answer(tool, &["auth", "status"], false).map(|(ok, _)| ok),
    };
    ToolReport {
        tool,
        version,
        signed_in,
    }
}

/// Runs `tool` with `args` and no input, waiting at most [`REPORT_TIMEOUT`]: whether it
/// succeeded, and its standard output when `keep` (else it goes nowhere). `None` when it
/// couldn't be run or took too long. The output is read as it comes, so a long one can't stall
/// the tool.
fn report_answer(tool: &str, args: &[&str], keep: bool) -> Option<(bool, String)> {
    let mut cmd = tool_command(tool);
    cmd.args(args).current_dir(std::env::temp_dir());
    let options = auth_options(REPORT_TIMEOUT, keep).ok()?;
    finish_report(crate::processes::run_checked(cmd, options), keep)
}

fn finish_report(
    result: Result<crate::processes::Output, crate::processes::Failure>,
    keep: bool,
) -> Option<(bool, String)> {
    let out = result.ok()?.complete().ok()?;
    Some((
        out.status.success(),
        if keep {
            String::from_utf8_lossy(&out.stdout).into_owned()
        } else {
            String::new()
        },
    ))
}

pub(super) fn history_api(
    hosting: &super::Hosting,
    endpoint: &str,
    active: &dyn Fn() -> bool,
) -> Result<serde_json::Value, String> {
    let (tool, forge) = match hosting.forge {
        Forge::GitHub => ("gh", Forge::GitHub),
        Forge::GitLab => ("glab", Forge::GitLab),
        _ => return Err(crate::i18n::text("backend-this-forge-uses-its-http-api-60dbf1").into()),
    };
    let authority = hosting
        .web
        .split_once("://")
        .map(|(_, rest)| rest.trim_end_matches('/'))
        .ok_or(crate::i18n::text("backend-invalid-forge-address-0f6311"))?;
    if authority.contains(['/', '@', '\n', '\r'])
        || authority.starts_with('-')
        || endpoint.starts_with(['/', '-'])
        || endpoint.contains("://")
        || endpoint.chars().any(char::is_control)
    {
        return Err(crate::i18n::text("backend-invalid-history-api-address-f52ebe").into());
    }
    let actual = super::location(&format!("{}/_history_guard_", hosting.web))
        .map(|(host, _)| host)
        .ok_or(crate::i18n::text("backend-invalid-forge-authority-9d9b21"))?;
    if !actual.eq_ignore_ascii_case(&hosting.host) {
        return Err(crate::i18n::text(
            "backend-forge-account-and-api-authority-do-not-match-1393cc",
        )
        .into());
    }
    if !active() {
        return Err("History lookup cancelled".into());
    }
    let mut command = tool_command(tool);
    command
        .current_dir(std::env::temp_dir())
        .env_remove("GH_DEBUG")
        .env_remove("GLAB_DEBUG")
        .env_remove("GH_FORCE_TTY")
        .env("GH_PROMPT_DISABLED", "1");
    match forge {
        Forge::GitHub => {
            command.env("GH_REPO", format!("{}/{}", hosting.host, hosting.path));
        }
        Forge::GitLab => {
            command.env("GITLAB_HOST", authority);
        }
        _ => {}
    }
    command.args(["api", "--hostname", authority, endpoint]);
    let output = crate::processes::run(
        command,
        crate::processes::Options {
            background: true,
            timeout: Some(Duration::from_secs(15)),
            output_limit: 2 * 1024 * 1024,
            stderr_limit: Some(16 * 1024),
            ..Default::default()
        },
    )
    .and_then(|o| o.complete())
    .map_err(|_| "Bounded forge history lookup failed or was cancelled")?;
    if !active() {
        return Err("History lookup cancelled".into());
    }
    if !output.status.success() {
        return Err(crate::i18n::text(
            "backend-forge-history-lookup-failed-check-the-selected-account-and-server-ff8b19",
        )
        .into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| {
        crate::i18n::text("backend-forge-history-lookup-returned-malformed-json-faed17").into()
    })
}

#[cfg(test)]
mod backend_fluent_identity_tests;
/// Recording happens after command preparation, at the process boundary. No forge tool runs.
#[cfg(test)]
pub(super) mod recording;
#[cfg(test)]
mod tests;

mod auth;
use auth::*;

mod capped;
pub(super) use capped::run_capped;

mod routing;
pub(crate) use routing::tool_location;
use routing::*;
