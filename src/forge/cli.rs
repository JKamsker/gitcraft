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

/// The command that runs `tool` with `args` about `repo` (here, or named for one on another
/// machine), and what to say when it can't be started.
fn prepared(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
) -> Result<(Command, impl Fn(String) -> String), String> {
    let mut cmd = tool_command(tool);
    let forge = forge_of(tool)?;
    if forge != hosting.forge || !plain_path(&hosting.path) || !plain_host(&hosting.host) {
        return Err(crate::i18n::message(
            "integration-forge-needs-selected",
            &[("tool", tool.into())],
        ));
    }
    let authority = match forge {
        Forge::GitLab => super::gitlab::api_authority(hosting),
        _ => hosting.host.clone(),
    };
    if !plain_host(&authority) || !same_host(&authority, &hosting.host) {
        return Err(crate::i18n::text("integration-forge-invalid-web-host").to_owned());
    }
    let remote = crate::remote::parse(repo).is_some();
    let context = crate::connections::current();
    if let Some(error) = &context.error {
        return Err(error.clone());
    }
    crate::connections::configure_cli(&mut cmd, tool, &authority)?;
    if remote {
        let remotes = remote_catalog(repo)?;
        validate_remote_target(&remotes, hosting)?;
        // A named target bypasses the CLI's usual known-host filtering. Only an explicitly
        // configured login (never an inherited global token) may authorize an SSH-tab host.
        if !trusted_host(forge, tool, &authority)? {
            return Err(not_signed_in(tool, &authority));
        }
    }
    if named_hosts(args)
        .iter()
        .any(|host| !host.eq_ignore_ascii_case(&authority))
    {
        return Err(crate::i18n::text("integration-forge-different-host").to_owned());
    }
    let mut args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let api = args.first().is_some_and(|s| s == "api");
    // These switches are command-family specific. Neither tool's `api` accepts --repo.
    // Insert before `--`, which release uploads use to protect asset filenames.
    let at = args.iter().position(|s| s == "--").unwrap_or(args.len());
    if api {
        if !args.iter().any(|s| s == "--hostname") {
            args.splice(at..at, ["--hostname".to_owned(), authority.clone()]);
        }
    } else {
        let target = match forge {
            Forge::GitLab => format!("{}/{}", hosting.web.trim_end_matches('/'), hosting.path),
            _ => format!("{}/{}", hosting.host, hosting.path),
        };
        if args.iter().any(|s| matches!(s.as_str(), "--repo" | "-R")) {
            return Err(crate::i18n::text("integration-forge-no-repository-override").to_owned());
        }
        args.splice(at..at, ["--repo".to_owned(), target]);
    }
    // Hosting and --repo identify the selected repository explicitly. Running inside
    // its worktree would let a local .glab-cli/config.yml redirect the CLI anyway.
    cmd.current_dir(std::env::temp_dir()).args(args);
    match forge {
        Forge::GitLab => {
            cmd.env("GITLAB_HOST", &authority)
                .env("GITLAB_API_HOST", &authority);
        }
        _ => {
            cmd.env("GH_REPO", format!("{}/{}", hosting.host, hosting.path));
        }
    }
    let here = remote || repo.is_dir();
    let (tool, noun, shown) = (tool.to_owned(), noun.to_owned(), repo.display().to_string());
    // Service failures are strings, not typed spawn errors. Keep the original cause and an
    // installation hint for a failed launch, without guessing an OS errno or rebuilding PATH.
    let start_error = move |error: String| {
        let id = if here && error.starts_with("could not run process:") {
            "backend-forge-launch-install"
        } else {
            "backend-forge-launch-error"
        };
        let purpose = match noun.as_str() {
            "pull request" => crate::i18n::text("backend-forge-purpose-pull"),
            "merge request" => crate::i18n::text("backend-forge-purpose-merge"),
            "issue" => crate::i18n::text("backend-forge-purpose-issue"),
            other => other,
        };
        crate::i18n::message(
            id,
            &[
                ("tool", tool.as_str().into()),
                ("path", shown.as_str().into()),
                ("detail", error.into()),
                ("purpose", purpose.into()),
            ],
        )
    };
    Ok((cmd, start_error))
}

/// The forge whose tool `tool` is.
fn forge_of(tool: &str) -> Result<Forge, String> {
    match tool {
        "gh" => Ok(Forge::GitHub),
        "glab" => Ok(Forge::GitLab),
        _ => Err(crate::i18n::message(
            "backend-tool-can-t-be-run-for-a-repository-on-another-machine-966b8b",
            &[("tool", (tool).to_string().into())],
        )),
    }
}

/// Whether `glab` with `args` takes `--repo`: every command but `api`.
#[cfg(test)]
fn gitlab_takes_repo(args: &[&str]) -> bool {
    args.first().is_some_and(|sub| *sub != "api")
}

/// Re-read the SSH repository's catalog before each command; selection is a host AND path,
/// rather than an implicit origin. A changed/removed URL requires refreshing the tab.
fn remote_catalog(repo: &Path) -> Result<Vec<crate::git::Remote>, String> {
    #[cfg(test)]
    if let Some(remotes) = recording::remotes() {
        return Ok(remotes);
    }
    crate::git::remotes(repo)
}

fn validate_remote_target(remotes: &[crate::git::Remote], hosting: &Hosting) -> Result<(), String> {
    if remotes
        .iter()
        .any(|remote| hosting.matches_git_remote(&remote.url))
    {
        Ok(())
    } else {
        Err(crate::i18n::text("integration-forge-remote-changed").to_owned())
    }
}

fn plain_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with(['-', '.', ':'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

// The inbox's retry policy consumes this protocol identity; translate at display_error.
fn not_signed_in(tool: &str, host: &str) -> String {
    format!("{tool} isn't signed in to {host}: `{tool} auth login --hostname {host}` to use it")
}

/// Whether `path` is a plain `owner/repo` (`group/sub/repo`): never a URL, an `scp`-style
/// address or an option, which `glab --repo` would take as another host.
fn plain_path(path: &str) -> bool {
    path.contains('/')
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !matches!(part, "." | "..")
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// The hosts `args` name: the value of `--hostname`, and the host of a `-R` / `--repo` of the
/// form `host/owner/repo`.
fn named_hosts(args: &[&str]) -> Vec<String> {
    let mut hosts = Vec::new();
    for pair in args.windows(2) {
        match pair[0] {
            "--hostname" => hosts.push(pair[1].to_owned()),
            "-R" | "--repo" if pair[1].matches('/').count() >= 2 => {
                hosts.push(pair[1].split('/').next().unwrap_or_default().to_owned());
            }
            _ => {}
        }
    }
    hosts
}

/// Whether two hosts are the same, ports aside (`glab api --hostname` takes none).
fn same_host(a: &str, b: &str) -> bool {
    let name = |h: &str| h.split(':').next().unwrap_or(h).to_lowercase();
    name(a) == name(b)
}

/// The host and `owner/repo` the tool is given for remote `url`: GitLab's host with the port of
/// an `http(s)` URL, an SSH port left out (it says nothing of the web pages).
pub(crate) fn tool_location(forge: Forge, url: &str) -> Option<(String, String)> {
    let (host, path) = super::location(url)?;
    let host = match (forge, http_port(url)) {
        (Forge::GitLab, Some(port)) => format!("{host}:{port}"),
        _ => host,
    };
    Some((host, path))
}

/// The port of an `http(s)` URL that names one.
fn http_port(url: &str) -> Option<&str> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split('/').next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (_, port) = host.rsplit_once(':')?;
    (!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())).then_some(port)
}

/// How long the tool may take to say whether it is signed in to a host.
const AUTH_CHECK_FOR: Duration = Duration::from_secs(10);
/// How long a host the tool isn't signed in to is taken as such before it is asked again (the
/// user may sign in meanwhile).
const NOT_SIGNED_IN_FOR: Duration = Duration::from_secs(60);

/// Whether `forge`'s tool may be pointed at `host`: the forge's own host, or one the tool is
/// signed in to. A host that isn't a plain name (`a.b:8443` at most) never is. What the tool
/// said is remembered: yes while the app runs, no for [`NOT_SIGNED_IN_FOR`].
/// An error when the tool can't be run at all (not installed).
fn trusted_host(forge: Forge, tool: &str, host: &str) -> Result<bool, String> {
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
fn signed_in_to(tool: &str, host: &str) -> Result<bool, String> {
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

fn finish_auth(
    result: Result<crate::processes::Output, crate::processes::Failure>,
) -> Result<bool, String> {
    result
        .map_err(crate::processes::Failure::message)?
        .complete()
        .map(|out| out.status.success())
}

#[cfg(windows)]
const NULL_SINK: &str = "NUL";
#[cfg(not(windows))]
const NULL_SINK: &str = "/dev/null";

fn auth_options(within: Duration, keep: bool) -> Result<crate::processes::Options, String> {
    auth_options_with(within, keep, |path| {
        std::fs::OpenOptions::new().write(true).open(path)
    })
}

fn auth_options_with(
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
mod tests {
    #[test]
    fn review13_ssh_catalog_check_keeps_web_authority_prefix_and_forge_case() {
        for forge in [Forge::GitHub, Forge::GitLab] {
            let h = Hosting {
                forge,
                host: "forge.example".into(),
                path: "Team/Repo".into(),
                web: "https://forge.example:4000/reviews".into(),
            };
            for (url, expected) in [
                ("https://forge.example:3000/reviews/Team/Repo.git", false),
                ("https://forge.example:4000/Team/Repo.git", false),
                ("https://forge.example:4000/REVIEWS/Team/Repo.git", false),
                ("https://FORGE.example:4000/reviews/Team/Repo.git", true),
                ("git@forge.example:Team/Repo.git", true),
                ("ssh://git@forge.example:2222/Team/Repo.git", true),
                ("git@forge.example:team/repo.git", forge != Forge::GitLab),
            ] {
                let remotes = [crate::git::Remote {
                    name: "origin".into(),
                    url: url.into(),
                }];
                assert_eq!(
                    validate_remote_target(&remotes, &h).is_ok(),
                    expected,
                    "{forge:?} {url}"
                );
            }
        }
    }

    use super::*;

    /// A self-hosted GitLab on a port of its own is reached on it; an SSH port isn't a web one.
    #[test]
    fn gitlab_keeps_the_port_of_its_web_pages() {
        let at = |forge, url| tool_location(forge, url);
        assert_eq!(
            at(Forge::GitLab, "https://git.example.org:8443/team/app.git"),
            Some(("git.example.org:8443".to_owned(), "team/app".to_owned()))
        );
        assert_eq!(
            at(Forge::GitLab, "ssh://git@git.example.org:2222/team/app.git"),
            Some(("git.example.org".to_owned(), "team/app".to_owned()))
        );
        assert_eq!(
            at(Forge::GitLab, "git@gitlab.com:team/app.git"),
            Some(("gitlab.com".to_owned(), "team/app".to_owned()))
        );
        // `GH_REPO` takes a plain host.
        assert_eq!(
            at(Forge::GitHub, "https://ghe.example.org:8443/team/app"),
            Some(("ghe.example.org".to_owned(), "team/app".to_owned()))
        );
    }

    /// A repository's remote names a plain project, never another host by way of its path.
    #[test]
    fn only_plain_paths_reach_the_tools() {
        assert!(plain_path("team/app"));
        assert!(plain_path("group/sub/my.app"));
        for bad in [
            "https://evil.example/x/y",
            "git@evil.example:x/y",
            "other.corp/g/p:1",
            "-x/y",
            "app",
            "a//b",
        ] {
            assert!(!plain_path(bad), "{bad}");
        }
        let (_, path) = tool_location(
            Forge::GitLab,
            "https://gitlab.com/https://evil.example/x/y.git",
        )
        .unwrap_or_default();
        assert!(!plain_path(&path), "{path}");
    }

    /// The hosts the arguments send the tool to, which must be the remote's own.
    #[test]
    fn the_hosts_arguments_name_are_found() {
        assert_eq!(
            named_hosts(&["api", "graphql", "--hostname", "ghe.example.org", "-f", "q"]),
            ["ghe.example.org"]
        );
        assert_eq!(
            named_hosts(&["run", "list", "-R", "ghe.example.org/o/r"]),
            ["ghe.example.org"]
        );
        assert!(named_hosts(&["run", "list", "-R", "o/r"]).is_empty());
        assert!(same_host("git.example.org:8443", "git.example.org"));
        assert!(!same_host("github.evil.example", "github.com"));
    }

    /// A tool that hangs is stopped at its time; a long answer still comes whole.
    #[cfg(unix)]
    #[test]
    fn a_tool_given_a_time_is_stopped_at_it() {
        let run_within = |tool: &str, _: &str, here: &Path, args: &[&str], within| {
            let mut cmd = tool_command(tool);
            cmd.current_dir(here).args(args);
            within_command(tool, cmd, |e| e, within)
        };
        let here = std::env::temp_dir();
        let started = Instant::now();
        let hung = run_within("sleep", "x", &here, &["20"], Duration::from_millis(300));
        assert!(hung.is_err_and(|e| e.contains("didn't finish within")));
        assert!(started.elapsed() < Duration::from_secs(10));
        let script = "head -c 300000 /dev/zero | tr '\\0' x";
        let long = run_within("sh", "x", &here, &["-c", script], Duration::from_secs(20));
        assert_eq!(long.map(|out| out.len()), Ok(300_000));
        let failed = run_within(
            "sh",
            "x",
            &here,
            &["-c", "echo no >&2; exit 3"],
            Duration::from_secs(20),
        );
        assert_eq!(failed, Err("no".to_owned()));
    }

    /// `glab api` would refuse `--repo`; the commands about a project take it.
    #[test]
    fn only_glab_commands_about_a_project_take_repo() {
        assert!(!gitlab_takes_repo(&[
            "api",
            "--hostname",
            "gitlab.com",
            "projects/x"
        ]));
        assert!(gitlab_takes_repo(&["mr", "list", "-F", "json"]));
        assert!(!gitlab_takes_repo(&[]));
    }

    /// The user's environment can't colour the JSON read from the tools, nor point them at
    /// another repository or host.
    #[test]
    fn tools_answer_in_plain_json_about_this_repository() {
        for tool in ["gh", "glab"] {
            let cmd = tool_command(tool);
            let envs: HashMap<String, Option<String>> = cmd
                .get_envs()
                .map(|(k, v)| {
                    let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
                    (text(k), v.map(text))
                })
                .collect();
            for removed in [
                "CLICOLOR_FORCE",
                "GH_FORCE_TTY",
                "GH_REPO",
                "GH_HOST",
                "GITLAB_HOST",
                "GITLAB_REPO",
                "GITLAB_URI",
                "GL_HOST",
                "GITLAB_HEAD_REPO",
            ] {
                assert_eq!(envs.get(removed), Some(&None), "{removed}");
            }
            assert_eq!(envs.get("NO_COLOR"), Some(&Some("1".to_owned())));
        }
    }

    #[test]
    fn bounded_requests_keep_input_deadline_and_ambient_cancellation() -> Result<(), String> {
        let cancel = crate::tasks::Cancellation::default();
        crate::tasks::with_cancellation(cancel.clone(), || {
            let options = bounded_options(Duration::from_secs(7), Some("a body\n"));
            cancel.cancel();
            let mut command = Command::new("/recorded/gh");
            command
                .args(["api", "endpoint"])
                .current_dir("/recorded/repo");
            command.env("GH_HOST", "recorded.example");
            let result = dispatch(
                command,
                options,
                |error| format!("start: {error}"),
                |command, options| {
                    assert_eq!(command.get_program(), "/recorded/gh");
                    assert_eq!(command.get_args().collect::<Vec<_>>(), ["api", "endpoint"]);
                    assert_eq!(command.get_current_dir(), Some(Path::new("/recorded/repo")));
                    assert_eq!(
                        command.get_envs().collect::<Vec<_>>(),
                        [(
                            std::ffi::OsStr::new("GH_HOST"),
                            Some(std::ffi::OsStr::new("recorded.example"))
                        )]
                    );
                    assert_eq!(options.input.as_deref(), Some(b"a body\n".as_slice()));
                    assert_eq!(options.timeout, Some(Duration::from_secs(7)));
                    assert_eq!(options.output_limit, OUTPUT_LIMIT);
                    assert_eq!(options.stderr_limit, Some(ERROR_LIMIT));
                    assert!(!options.hold_input);
                    assert!(options.cancel.is_cancelled());
                    Err(crate::processes::Failure::CancelledBeforeStart)
                },
            );
            assert!(matches!(result, Err(ref error) if error == "cancelled before execution"));
        });
        assert_eq!(TOOL_TIMEOUT, Duration::from_secs(120));
        Ok(())
    }

    #[test]
    fn stopped_or_uncertain_results_never_become_success_or_authentication() {
        use crate::processes::{StopKind, recording_output};
        for kind in [
            StopKind::Cancelled,
            StopKind::TimedOut,
            StopKind::Failed,
            StopKind::Uncertain,
        ] {
            let result = finish_run(
                "gh",
                Duration::from_secs(3),
                Ok(recording_output(Some(kind))),
            );
            assert!(result.is_err());
            if kind == StopKind::Uncertain {
                assert_eq!(result, Err("cleanup proof missing".into()));
            }
            if kind == StopKind::TimedOut {
                assert!(
                    result.is_err_and(|error| error.contains("didn't finish within 3 seconds"))
                );
            }
            assert!(finish_auth(Ok(recording_output(Some(kind)))).is_err());
            assert!(finish_report(Ok(recording_output(Some(kind))), true).is_none());
        }
        assert_eq!(finish_auth(Ok(recording_output(None))), Ok(true));
    }

    #[test]
    fn tool_exit_diagnostics_survive_and_discarded_reports_reveal_no_output() {
        use crate::processes::recording_output;
        let mut output = recording_output(None);
        output.stdout = b"tool response".to_vec();
        assert_eq!(
            finish_run("gh", TOOL_TIMEOUT, Ok(output)),
            Ok("tool response".into())
        );
        #[cfg(unix)]
        let failed_status = {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(3 << 8)
        };
        #[cfg(windows)]
        let failed_status = {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(3)
        };
        let mut failed = recording_output(None);
        failed.status = failed_status;
        failed.stderr = b"actual exit diagnostic".to_vec();
        assert_eq!(
            finish_run("gh", TOOL_TIMEOUT, Ok(failed)),
            Err("actual exit diagnostic".into())
        );
        for success in [false, true] {
            let mut output = recording_output(None);
            if !success {
                output.status = failed_status;
            }
            output.stdout = b"secret token".to_vec();
            output.stderr = b"secret account".to_vec();
            assert_eq!(
                finish_report(Ok(output), false),
                Some((success, String::new()))
            );
        }
    }

    #[test]
    fn auth_capture_is_bounded_and_redacts_stderr_before_retention() -> Result<(), String> {
        // keep=true does not open a sink or launch a program. The production keep=false path
        // uses stdout_file, since Command.stdout would be overridden by Service.
        let mut options = auth_options(REPORT_TIMEOUT, true)?;
        assert_eq!(options.timeout, Some(REPORT_TIMEOUT));
        assert_eq!(options.output_limit, ERROR_LIMIT);
        assert_eq!(options.stderr_limit, Some(ERROR_LIMIT));
        assert!(options.stdout_file.is_none());
        assert!(options.input.is_none());
        let filter = options.stderr_filter.as_mut().ok_or("missing redaction")?;
        assert_eq!(filter(b"secret token"), vec![0; 12]);
        assert_eq!(filter(b""), Vec::<u8>::new());
        assert_eq!(filter(&vec![b's'; ERROR_LIMIT + 1]).len(), ERROR_LIMIT + 1);
        Ok(())
    }

    #[test]
    fn secret_discard_requires_null_sink_and_refuses_failed_open() {
        // Recording opener: never opens a file or starts the service. A sink failure must
        // refuse construction rather than falling back to captured token stdout.
        let opened = std::cell::Cell::new(false);
        let result = auth_options_with(REPORT_TIMEOUT, false, |path| {
            opened.set(true);
            assert_eq!(path, NULL_SINK);
            #[cfg(unix)]
            assert_eq!(path, "/dev/null");
            #[cfg(windows)]
            assert_eq!(path, "NUL");
            Err(std::io::Error::other("recorded sink refusal"))
        });
        assert!(opened.get());
        assert!(result.is_err_and(|error| error.contains("recorded sink refusal")));
        assert!(
            auth_options_with(REPORT_TIMEOUT, true, |_| {
                opened.set(false);
                Err(std::io::Error::other("unexpected open"))
            })
            .is_ok()
        );
        assert!(opened.get());
    }

    #[test]
    fn oversized_input_refuses_before_command_preparation() {
        // An invalid path/tool would reach discovery if preparation ran. The input guard is
        // first, so this is a pure rejection test with no filesystem or process access.
        let input = "x".repeat(INPUT_LIMIT + 1);
        assert_eq!(
            run_with_input(
                "/recorded/gh",
                "review",
                Path::new("/recorded/repo"),
                &Hosting {
                    forge: Forge::GitHub,
                    host: "github.com".into(),
                    path: "o/r".into(),
                    web: "https://github.com".into()
                },
                &[],
                Some(&input)
            ),
            Err(format!(
                "/recorded/gh input exceeds {} bytes",
                crate::i18n::number(INPUT_LIMIT)
            )),
        );
    }
}

#[cfg(test)]
mod backend_fluent_identity_tests {
    #[test]
    fn backend_fluent_signin_keeps_retry_identity() {
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            let raw = super::not_signed_in("glab", "git.example:8443");
            assert_eq!(
                raw,
                "glab isn't signed in to git.example:8443: `glab auth login --hostname git.example:8443` to use it"
            );
            assert!(crate::forge::inbox::gives_up(&raw));
            let display = crate::forge::inbox::display_error(&raw);
            if locale == "de" {
                assert_eq!(
                    display,
                    "glab ist nicht bei git.example:8443 angemeldet: zur Verwendung `glab auth login --hostname git.example:8443` ausführen."
                );
            } else {
                assert_eq!(display, raw);
            }
            for opaque in [
                "provider says it isn't signed in to elsewhere",
                "provider isn't signed in to host: `provider auth login --hostname host` to use it",
            ] {
                assert_eq!(crate::forge::inbox::display_error(opaque), opaque);
            }
        }
        crate::i18n::set_language("en-US");
    }
}
/// Recording happens after command preparation, at the process boundary. No forge tool runs.
#[cfg(test)]
pub(super) mod recording {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::process::Command;

    #[derive(Debug)]
    pub struct Call {
        pub tool: String,
        pub args: Vec<String>,
        pub env: Vec<(String, Option<String>)>,
        pub input: Option<String>,
    }
    #[derive(Default)]
    struct Recording {
        calls: Vec<Call>,
        answers: VecDeque<Result<String, String>>,
        remotes: Option<Vec<crate::git::Remote>>,
    }
    thread_local! {
        static ACTIVE: RefCell<Option<Recording>> = const { RefCell::new(None) };
    }
    pub fn run(
        answers: Vec<Result<String, String>>,
        work: impl FnOnce() -> Result<(), String>,
    ) -> Result<Vec<Call>, String> {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                ACTIVE.with(|a| *a.borrow_mut() = None);
            }
        }
        ACTIVE.with(|a| {
            assert!(a.borrow().is_none());
            *a.borrow_mut() = Some(Recording {
                answers: answers.into(),
                ..Default::default()
            });
        });
        let _reset = Reset;
        work()?;
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            let recording = a.take().ok_or("missing active recorder")?;
            assert!(recording.answers.is_empty(), "unconsumed CLI responses");
            Ok(recording.calls)
        })
    }
    pub fn set_remotes(remotes: Vec<crate::git::Remote>) {
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            assert!(a.is_some(), "missing recorder");
            if let Some(r) = a.as_mut() {
                r.remotes = Some(remotes);
            }
        });
    }
    pub(super) fn remotes() -> Option<Vec<crate::git::Remote>> {
        ACTIVE.with(|a| a.borrow().as_ref().and_then(|r| r.remotes.clone()))
    }
    pub(super) fn answer(cmd: &Command, input: Option<&str>) -> Option<Result<String, String>> {
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            let recording = a.as_mut()?;
            recording.calls.push(Call {
                tool: cmd.get_program().to_string_lossy().into_owned(),
                args: cmd
                    .get_args()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
                env: cmd
                    .get_envs()
                    .map(|(k, v)| {
                        (
                            k.to_string_lossy().into_owned(),
                            v.map(|v| v.to_string_lossy().into_owned()),
                        )
                    })
                    .collect(),
                input: input.map(str::to_owned),
            });
            assert!(!recording.answers.is_empty(), "unexpected forge invocation");
            Some(
                recording
                    .answers
                    .pop_front()
                    .unwrap_or_else(|| Err("unexpected forge invocation".into())),
            )
        })
    }
}
