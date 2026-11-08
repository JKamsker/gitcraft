//! Running git: the environment every call gets, credentials for remotes, errors. A call for a
//! repository in WSL or over SSH runs git there (see [`crate::remote`]), with the same
//! environment.

pub(super) mod bounded;
mod source;
pub use source::CommandSource;

use std::path::{Path, PathBuf};
use std::process::Command;
/// The initiating task's immutable profile config (including queued and nested reads).
pub fn profile_config() -> Option<PathBuf> {
    crate::connections::current().git_config
}

/// Whether `args` reads or writes the user's own global config: `git config --global …`
/// (signing settings, Mergiraf's set-up) or `git maintenance` (its list of repositories), which
/// must never land in the profile's file, which is written over.
fn user_global_config(args: &[&str]) -> bool {
    let rest = after_options(args);
    (rest.first() == Some(&"config") && rest.contains(&"--global"))
        // `maintenance start` / `register` write `maintenance.repo` to the global config.
        || (rest.first() == Some(&"maintenance")
            && matches!(rest.get(1), Some(&("start" | "register" | "unregister" | "stop"))))
}

/// `args` from the subcommand on: past the options before it (`-c x=y`, `-C dir`,
/// `--literal-pathspecs`…).
pub(super) fn after_options<'a>(args: &'a [&'a str]) -> &'a [&'a str] {
    let mut rest = args;
    loop {
        match rest {
            ["-c" | "-C", _, tail @ ..] => rest = tail,
            [word, tail @ ..] if word.starts_with('-') => rest = tail,
            _ => return rest,
        }
    }
}

/// Points `cmd` at the active profile's config, unless `args` is about the user's own.
pub(super) fn with_profile(cmd: &mut Command, args: &[&str]) {
    if user_global_config(args) {
        return;
    }
    let context = crate::connections::current();
    if context.error.is_some() {
        cmd.arg("-c").arg("=invalid-profile");
        return;
    }
    if !context.choices.ssh_key.is_empty() {
        cmd.env_remove("GIT_SSH_COMMAND")
            .env_remove("GIT_SSH")
            .env_remove("GIT_SSH_VARIANT");
    }
    let path = context.git_config;
    if let Some(path) = path {
        cmd.env("GIT_CONFIG_GLOBAL", path);
    }
}

use crate::remote::{self, Target};

mod connection;
mod credential_policy;
mod endpoints;
#[cfg(all(test, unix))]
mod native_https_fixture;
pub(crate) use credential_policy::*;
pub(crate) use endpoints::*;
mod setup;
pub(crate) use connection::*;
pub(crate) use setup::{setup_clone, setup_fetch, setup_path, setup_run};
mod watched;
pub use watched::{CANCELLED, OpControl, run_watched, run_watched_recorded};

/// What every git call is run with.
/// - `GIT_TERMINAL_PROMPT`: never block on an interactive prompt (credential helpers with a
///   GUI still work).
/// - `GIT_EDITOR`: `rebase --continue`, `merge --continue`… would otherwise open an editor
///   nobody sees.
/// - `LANGUAGE`: messages in English whatever the user's language: errors are told apart by
///   their words (`is_auth_error`…). `C` in LANGUAGE wins over LC_ALL / LANG for messages only,
///   so the character set (and what hooks see of it) stays the user's.
pub(super) const GIT_ENV: [(&str, &str); 4] = [
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_EDITOR", "true"),
    ("LANGUAGE", "C"),
];

/// Interactive rebases set `sequence.editor` themselves; an inherited one would win.
pub(super) const GIT_ENV_REMOVED: [&str; 1] = ["GIT_SEQUENCE_EDITOR"];

/// A tab's explicit repository path selects its repository for both guards and
/// mutations. Inherited shell plumbing must not silently select another index,
/// namespace, object store or shallow boundary. Deliberate per-command overrides
/// are installed afterwards through `Extra::env` (private indexes, for example).
pub(super) const REPOSITORY_ENV_REMOVED: &[&str] = &[
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_SHALLOW_FILE",
    "GIT_REPLACE_REF_BASE",
    "GIT_GRAFT_FILE",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_ATTR_SOURCE",
];

/// Pure environment portion of the shared selected-repository command builder.
fn clear_repository_environment(cmd: &mut Command) {
    for name in REPOSITORY_ENV_REMOVED {
        cmd.env_remove(name);
    }
}

pub(super) fn git_cmd() -> Command {
    // With a bare name and an overridden PATH, Rust falls back to fork/exec on Unix.
    // Resolve through the child's PATH first, allowing its much cheaper posix_spawn path.
    // Look up each time so installing or removing a git still takes effect while running.
    #[cfg(unix)]
    let mut cmd =
        Command::new(crate::shell_path::executable("git").unwrap_or_else(|| "git".into()));
    #[cfg(not(unix))]
    let mut cmd = Command::new("git");
    for (name, value) in GIT_ENV {
        cmd.env(name, value);
    }
    for name in GIT_ENV_REMOVED {
        cmd.env_remove(name);
    }
    clear_repository_environment(&mut cmd);
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// How a git call ended, here or elsewhere.
struct Ran {
    code: Option<i32>,
    /// The exit status as it reads in an error.
    status: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// The hook that failed, when the call's hooks were traced.
    failed_hook: Option<super::hooks::FailedHook>,
}

/// Options before the subcommand, and variables, a call is run with on top of [`GIT_ENV`].
#[derive(Clone, Default)]
pub(super) struct Extra {
    /// Normalize before installing intentional command-owned environment.
    normalize_repository: bool,
    unset: Vec<&'static str>,
    output_limit: Option<usize>,
    stdout_file: Option<PathBuf>,
    pre: Vec<&'static str>,
    owned_pre: Vec<String>,
    env: Vec<(&'static str, String)>,
    /// What git reads on its input; nothing when `None`.
    stdin: Option<Vec<u8>>,
    /// It may take minutes (a large bundle): on another machine, run where that is allowed.
    long: bool,
    /// It starts a program the user works in (a diff or merge tool) and waits until it closes.
    tool: bool,
    /// Find out which hook failed, if one does (see [`super::hooks`]).
    trace_hooks: bool,
    /// On another machine: a test of a parallel bisect (see [`run_for_test_input`]).
    test: Option<std::time::Duration>,
    /// How long it may run on another machine before it is stopped there.
    timeout: Option<std::time::Duration>,
}

impl Extra {
    /// Selected-repository inspection/mutation; owned private overrides are added
    /// after normalization by the shared production command builders.
    pub(in crate::git) fn repository() -> Self {
        Self {
            normalize_repository: true,
            ..Self::default()
        }
    }
}

pub(in crate::git) use watched::run_watched_extra_recorded;

/// How long a diff or merge tool started on another machine may stay open before it is stopped.
const TOOL_FOR: std::time::Duration = std::time::Duration::from_secs(12 * 3600);

/// Runs git with `args` in `dir` (on whichever machine it is), `extra` before them.
fn execute(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    let _timer = super::backend::CliTimer::new(args);
    let mut extra = extra.clone();
    let _source = source::bind(&mut extra, dir, args)?;
    let extra = &extra;
    if let Some(target) = dir.and_then(remote::parse) {
        let result = execute_there(&target, extra, args);
        if let Ok(out) = &result {
            source::finish(_source.as_ref(), args, out.code == Some(0))?;
            super::doctor::hook_log::record(dir, args, out.code == Some(0), &out.stderr);
        }
        return result;
    }
    let mut extra = extra.clone();
    let trace = extra
        .trace_hooks
        .then(super::hooks::TraceFile::new)
        .flatten();
    if let Some(trace) = &trace {
        extra.env.push((
            super::hooks::TRACE_VAR,
            trace.path().to_string_lossy().into_owned(),
        ));
    }
    let cmd = local_command(dir, &extra, args);
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            input: extra.stdin.clone(),
            stdout_file: extra
                .stdout_file
                .as_ref()
                .map(std::fs::File::create)
                .transpose()
                .map_err(|e| {
                    crate::i18n::message(
                        "backend-could-not-save-output-e-d0d53d",
                        &[("detail", (e).to_string().into())],
                    )
                })?,
            output_limit: extra.output_limit.unwrap_or(64 << 20),
            background: extra.long || extra.tool || extra.test.is_some() || goes_over_network(args),
            timeout: extra.timeout,
            ..Default::default()
        },
    )?
    .complete()?;
    source::finish(_source.as_ref(), args, out.status.success())?;
    super::doctor::hook_log::record(dir, args, out.status.success(), &out.stderr);
    let failed = !out.status.success();
    Ok(Ran {
        code: out.status.code(),
        status: out.status.to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        failed_hook: trace.filter(|_| failed).and_then(|t| t.failed_hook(args)),
    })
}

/// git with `args` in `dir` on this machine, `extra` before them.
pub(super) fn local_command(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Command {
    apply_command_extras(repository_command(dir, args), dir, extra, args)
}
/// Apply the actual production argv/environment policy to an already selected command.
/// Recording callers can provide a fixed executable without PATH discovery.
pub(super) fn apply_command_extras(
    mut cmd: Command,
    dir: Option<&Path>,
    extra: &Extra,
    args: &[&str],
) -> Command {
    for name in &extra.unset {
        cmd.env_remove(name);
    }
    if extra.normalize_repository && !dir.is_some_and(crate::practice::is_repository) {
        normalize_connection_command(&mut cmd, args);
    }
    cmd.args(&extra.pre).args(&extra.owned_pre);
    for (name, value) in &extra.env {
        cmd.env(name, value);
    }
    if extra
        .owned_pre
        .iter()
        .any(|arg| arg == "gitcraft-credential-transport")
    {
        let selected = cmd.get_program().to_owned();
        cmd.env("GITCRAFT_GIT_ORIGINAL", selected);
    }
    cmd.args(args);
    cmd
}

pub(super) const CONNECTION_REDIRECT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_NOSYSTEM",
];
fn normalize_connection_command(cmd: &mut Command, args: &[&str]) {
    for name in CONNECTION_REDIRECT_ENV {
        cmd.env_remove(name);
    }
    for (name, _) in std::env::vars_os() {
        let text = name.to_string_lossy();
        if text.starts_with("GIT_CONFIG_KEY_") || text.starts_with("GIT_CONFIG_VALUE_") {
            cmd.env_remove(name);
        }
    }
    // Intentional immutable profile config is reinstalled after inherited config is removed.
    with_profile(cmd, args);
}

/// The request that runs git with `args` in `target`'s folder, on its machine.
pub(super) fn remote_request(target: &Target, extra: &Extra, args: &[&str]) -> remote::Request {
    let guard = super::clone_options::passive_object_read(args).then_some("--no-lazy-fetch");
    let words: Vec<String> = guard
        .into_iter()
        .chain(
            extra
                .pre
                .iter()
                .copied()
                .chain(extra.owned_pre.iter().map(String::as_str))
                .chain(args.iter().copied()),
        )
        .map(remote::sh_quote)
        .collect();
    let credential_transport = extra
        .owned_pre
        .iter()
        .any(|arg| arg == "gitcraft-credential-transport");
    let pin = if credential_transport {
        CREDENTIAL_REMOTE_PIN
    } else {
        ""
    };
    let script = if extra.trace_hooks {
        let program = if credential_transport {
            "\"$GITCRAFT_GIT_ORIGINAL\""
        } else {
            "git"
        };
        format!(
            "{pin}{}",
            super::hooks::traced_script_with_program(&words.join(" "), program)
        )
    } else if credential_transport {
        format!("{pin}exec \"$GITCRAFT_GIT_ORIGINAL\" {}", words.join(" "))
    } else {
        format!("exec git {}", words.join(" "))
    };
    remote::Request {
        cwd: Some(target.path.clone()),
        script,
        env: GIT_ENV
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .chain(std::iter::once((
                "GIT_NO_LAZY_FETCH".to_owned(),
                super::clone_options::download_environment(args).to_owned(),
            )))
            .chain(extra.env.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .collect(),
        unset: GIT_ENV_REMOVED
            .iter()
            .chain(REPOSITORY_ENV_REMOVED.iter())
            .chain(
                extra
                    .normalize_repository
                    .then_some(CONNECTION_REDIRECT_ENV)
                    .into_iter()
                    .flatten(),
            )
            .chain(extra.unset.iter())
            .map(|n| (*n).to_owned())
            .collect(),
        stdin: extra.stdin.clone(),
        // A tool stays open as long as the user works in it, apart from ordinary git reads.
        long: extra.tool || extra.long || goes_over_network(args),
        timeout: extra
            .timeout
            .or(extra.test)
            .or_else(|| extra.tool.then_some(TOOL_FOR)),
        test: extra.test.is_some(),
        apart: false,
    }
}

/// The git alias a command of the user's (a conflict decision's test) is run as, set with
/// `-c alias.<it>=!<command>`: git runs it with `sh` in the repository's top folder, wherever
/// the repository is.
pub const USER_COMMAND_ALIAS: &str = "gitcraft-user-command";

/// Whether the git command `args` (its options first) talks to a remote and may take minutes
/// (or is a command of the user's, which may too).
pub fn goes_over_network(args: &[&str]) -> bool {
    matches!(
        after_options(args).first().copied(),
        Some(
            "fetch"
                | "pull"
                | "push"
                | "clone"
                | "ls-remote"
                | "remote"
                | "submodule"
                | "lfs"
                | SHELL_ALIAS
                | USER_COMMAND_ALIAS
                | super::dvc::PULL_ALIAS
        )
    )
}

/// The alias a shell script is run through (see [`run_shell`]).
const SHELL_ALIAS: &str = "gitcraft-run";

/// Defines [`SHELL_ALIAS`]: git runs a `!` alias with its own `sh` (Git for Windows' on
/// Windows), in the top folder of the working tree, with the script as its one argument.
const SHELL_ALIAS_DEF: &str = "alias.gitcraft-run=!f() { eval \"$1\"; }; f";

/// [`SHELL_ALIAS_DEF`] for a script handed over in [`SHELL_SCRIPT_VAR`]: in the environment,
/// not on a command line, where anyone on that machine could read it (`ps`) with the answers
/// it holds.
const SHELL_ALIAS_ENV_DEF: &str = "alias.gitcraft-run=!eval \"$GITCRAFT_SCRIPT\"";

/// The variable [`run_shell`] hands its script over in.
const SHELL_SCRIPT_VAR: &str = "GITCRAFT_SCRIPT";

/// How a shell script ended, and what it printed.
pub struct ShellRun {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs `script` with `sh` in the repository `dir`, on its machine (WSL, SSH: there), through
/// git, which has a `sh` wherever it is installed. What it needs to know goes in the script as
/// quoted variables, never into its text. A failing script is an `Ok` with its exit code;
/// `Err` only when it couldn't be run.
pub fn run_shell(dir: &Path, script: &str) -> Result<ShellRun, String> {
    let extra = Extra {
        pre: vec!["-c", SHELL_ALIAS_ENV_DEF],
        env: vec![(SHELL_SCRIPT_VAR, script.to_owned())],
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, &[SHELL_ALIAS])?;
    Ok(ShellRun {
        // Ended by a signal: no code, but not a success.
        code: out.code.unwrap_or(-1),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// The program and arguments that run `script` as [`run_shell`] does, in the repository `dir`
/// of this machine, for a terminal to run it in (where it can be watched and answered).
pub fn shell_command_here(dir: &Path, script: &str) -> (String, Vec<String>) {
    let args = [
        "-C".to_owned(),
        dir.display().to_string(),
        "-c".to_owned(),
        SHELL_ALIAS_DEF.to_owned(),
        SHELL_ALIAS.to_owned(),
        script.to_owned(),
    ];
    ("git".to_owned(), args.to_vec())
}

// Kept raw for recent-status/across classification. Translate only at display_error.
pub(super) fn missing_folder(target: &Target) -> String {
    format!("{} isn't a folder on {}", target.path, target.host.label())
}

fn execute_there(target: &Target, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    let out = remote::exec(&target.host, &remote_request(target, extra, args))?;
    // The folder is gone (or never was) on that machine.
    if out.code == 1 && remote::is_missing_folder(&out.stderr) {
        return Err(missing_folder(target));
    }
    let (stderr, events) = if extra.trace_hooks {
        super::hooks::split_trace(&out.stderr)
    } else {
        (out.stderr, String::new())
    };
    Ok(Ran {
        code: Some(out.code),
        status: crate::i18n::message(
            "backend-exit-status-c5b63a",
            &[("code", (out.code).to_string().into())],
        ),
        stdout: out.stdout,
        stderr,
        failed_hook: (out.code != 0)
            .then(|| super::hooks::failed_hook(&events, args))
            .flatten(),
    })
}

/// Runs git in `dir` and returns stdout, or the error output.
pub fn run(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    run_ok_codes(dir, args, &[0])
}

/// [`run`], with `input` on git's standard input (`notes add -F -`: a text of any size, which
/// a command line would limit).
pub fn run_input(dir: Option<&Path>, args: &[&str], input: &str) -> Result<String, String> {
    let extra = Extra {
        stdin: Some(input.as_bytes().to_vec()),
        ..Extra::default()
    };
    finish(execute(dir, &extra, args)?, args, &[0])
}

/// [`run`] for a read nobody asked for and nobody waits on (the start page's look at a recent
/// repository): for one on another machine, only in a shell already open there, so it never
/// connects (no password asked, no WSL started); `None` when there is none.
pub fn run_if_connected(dir: &Path, args: &[&str]) -> Option<Result<String, String>> {
    let Some(target) = remote::parse(dir) else {
        return Some(run(Some(dir), args));
    };
    let _timer = super::backend::CliTimer::new(args);
    let mut req = remote_request(&target, &Extra::default(), args);
    req.timeout = Some(std::time::Duration::from_secs(30));
    let out = match remote::exec_if_connected(&target.host, &req)? {
        Ok(out) => out,
        Err(e) => return Some(Err(e)),
    };
    Some(if out.code == 0 {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else if out.code == 1 && remote::is_missing_folder(&out.stderr) {
        Err(missing_folder(&target))
    } else {
        Err(download_read_error(
            args,
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    })
}

/// How a git call ended, whatever its exit code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    /// `None` when it was ended by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Stdout as git wrote it (`stdout` is it decoded, invalid UTF-8 replaced).
    pub raw: Vec<u8>,
}

/// Runs git in `dir` and returns how it ended, its exit code whatever it is, with `input` on
/// its standard input when given (`hash-object --stdin`…). Blocking: from the background.
pub fn run_captured(dir: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Captured, String> {
    let out = match (remote::parse(dir), input) {
        (Some(target), _) => {
            let _timer = super::backend::CliTimer::new(args);
            let mut request = remote_request(&target, &Extra::default(), args);
            request.stdin = input.map(<[u8]>::to_vec);
            let out = remote::exec(&target.host, &request)?;
            if out.code == 1 && remote::is_missing_folder(&out.stderr) {
                return Err(missing_folder(&target));
            }
            Ran {
                code: Some(out.code),
                status: crate::i18n::message(
                    "backend-exit-status-c5b63a",
                    &[("code", (out.code).to_string().into())],
                ),
                stdout: out.stdout,
                stderr: out.stderr,
                failed_hook: None,
            }
        }
        (None, None) => execute(Some(dir), &Extra::default(), args)?,
        (None, Some(input)) => run_here_with_input(dir, args, input)?,
    };
    Ok(Captured {
        code: out.code,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        raw: out.stdout,
    })
}

/// Runs git here in `dir` with `input` on its standard input.
fn run_here_with_input(dir: &Path, args: &[&str], input: &[u8]) -> Result<Ran, String> {
    execute(
        Some(dir),
        &Extra {
            stdin: Some(input.to_vec()),
            ..Extra::default()
        },
        args,
    )
}

/// [`run`] with variables of its own on top of what every call gets (`GIT_INDEX_FILE` for a
/// temporary index), on whichever machine the repository is.
pub fn run_env(
    dir: Option<&Path>,
    args: &[&str],
    env: &[(&'static str, &str)],
) -> Result<String, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: env.iter().map(|(n, v)| (*n, (*v).to_owned())).collect(),
        ..Extra::default()
    };
    let out = execute(dir, &extra, args)?;
    if out.code == Some(0) {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            crate::i18n::message(
                "backend-git-failed-da095f",
                &[
                    ("args", (args.join(" ")).to_string().into()),
                    ("status", (out.status).to_string().into()),
                ],
            )
        } else {
            stderr
        })
    }
}

/// Runs git and returns stdout as raw bytes (file contents: images…).
pub fn run_bytes(dir: Option<&Path>, args: &[&str]) -> Result<Vec<u8>, String> {
    // Blob callers already return a Vec, but need more headroom than command diagnostics.
    // Keep a finite per-read bound rather than allowing arbitrary process output in memory.
    let out = execute(
        dir,
        &Extra {
            output_limit: Some(512 << 20),
            ..Default::default()
        },
        args,
    )?;
    if out.code == Some(0) {
        Ok(out.stdout)
    } else {
        Err(download_read_error(
            args,
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

/// Bounded raw-object plumbing for source preparation before containment. This never
/// checks out files, invokes filters/hooks, or permits lazy object fetches/transports.
pub fn run_inert_objects(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    output_limit: usize,
    ok: &[i32],
) -> Result<Vec<u8>, String> {
    let extra = inert_object_options(args, input, output_limit)?;
    let out = execute(Some(dir), &extra, args)?;
    if out.code.is_some_and(|code| ok.contains(&code)) {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
fn inert_object_options(
    args: &[&str],
    input: Option<&[u8]>,
    output_limit: usize,
) -> Result<Extra, String> {
    if !matches!(
        args.first().copied(),
        Some("ls-tree" | "cat-file" | "rev-parse" | "hash-object" | "update-ref")
    ) {
        return Err(crate::i18n::text(
            "backend-sandbox-source-preparation-permits-only-raw-object-plumbing-c05abe",
        )
        .into());
    }
    Ok(Extra {
        pre: vec![
            "--no-pager",
            "--no-replace-objects",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
        ],
        env: vec![("GIT_NO_LAZY_FETCH", "1".into())],
        timeout: Some(std::time::Duration::from_secs(15)),
        stdin: input.map(<[u8]>::to_vec),
        output_limit: Some(output_limit),
        ..Default::default()
    })
}
#[cfg(test)]
mod r2_inert_tests {
    use super::*;
    #[test]
    fn r2_pure_source_plumbing_disables_hooks_transports_filters_and_has_a_deadline()
    -> Result<(), String> {
        let extra = inert_object_options(&["cat-file", "blob", "oid"], None, 8192)?;
        assert!(extra.pre.contains(&"core.hooksPath=/dev/null"));
        assert!(extra.pre.contains(&"protocol.allow=never"));
        assert!(extra.env.contains(&("GIT_NO_LAZY_FETCH", "1".into())));
        assert!(extra.timeout.is_some());
        assert_eq!(extra.output_limit, Some(8192));
        for command in [
            "checkout",
            "archive",
            "clone",
            "submodule",
            "clean",
            "fetch",
            "config",
        ] {
            assert!(inert_object_options(&[command], None, 1).is_err());
        }
        Ok(())
    }
}
/// Read-only pack generation for a bisect transfer. Standalone remote supervision keeps
/// the requested bounded binary capture independent of the pooled shell's 64 MiB frames.
/// Nonzero Git exits stay distinct from uncertain transport/cancellation errors.
pub(crate) fn transfer_pack(
    dir: &Path,
    revisions: Vec<u8>,
    thin: bool,
    limit: usize,
) -> Result<Exit, String> {
    let extra = Extra {
        stdin: Some(revisions),
        env: vec![("GIT_NO_LAZY_FETCH", "1".into())],
        timeout: Some(std::time::Duration::from_secs(1800)),
        ..Default::default()
    };
    let mut args = vec![
        "pack-objects",
        "--stdout",
        "--revs",
        "--delta-base-offset",
        "--missing=error",
    ];
    if thin {
        args.push("--thin");
    }
    let mut options = crate::processes::Options {
        input: extra.stdin.clone(),
        background: true,
        timeout: extra.timeout,
        output_limit: limit,
        ..Default::default()
    };
    let command = if let Some(target) = remote::parse(dir) {
        let request = remote_request(&target, &extra, &args);
        remote::streaming_command_bounded(&target.host, &request, None, None, false, limit)?
            .configure(&mut options)
    } else {
        local_command(Some(dir), &extra, &args)
    };
    let out = crate::processes::run(command, options)?.complete()?;
    Ok(Exit {
        code: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Save a command's stdout without capturing a local export in memory. Remote exports
/// retain the existing persistent-shell transport and its binary-output behavior.
pub fn run_to_file(dir: &Path, args: &[&str], file: &Path) -> Result<(), String> {
    if remote::is_remote(dir) {
        return run_bytes(Some(dir), args).and_then(|bytes| {
            std::fs::write(file, bytes).map_err(|e| {
                crate::i18n::message(
                    "backend-could-not-save-it-e-24f824",
                    &[("detail", (e).to_string().into())],
                )
            })
        });
    }
    let out = execute(
        Some(dir),
        &Extra {
            stdout_file: Some(file.to_owned()),
            ..Default::default()
        },
        args,
    )?;
    if out.code == Some(0) {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Runs git and returns its exit code with stdout as raw bytes, when the code is one of `ok`
/// (`merge-tree` answers on stdout and exits 1 on conflicts); else the error output.
pub fn run_code(dir: Option<&Path>, args: &[&str], ok: &[i32]) -> Result<(i32, Vec<u8>), String> {
    let out = execute(dir, &Extra::default(), args)?;
    match out.code {
        Some(code) if ok.contains(&code) => Ok((code, out.stdout)),
        _ => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(if stderr.is_empty() {
                crate::i18n::message(
                    "backend-git-failed-da095f",
                    &[
                        ("args", (args.join(" ")).to_string().into()),
                        ("status", (out.status).to_string().into()),
                    ],
                )
            } else {
                stderr
            })
        }
    }
}

/// How a diff or merge tool started through git ended.
#[derive(Debug)]
pub struct ToolRun {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `git difftool` / `git mergetool` (`args`) in `dir`, on whichever machine it is, and
/// waits for the tool it starts to close, however long it stays open (on another machine, at
/// most [`TOOL_FOR`]). Whatever git exits with is returned, not taken for an error: the caller
/// tells a cancelled merge from a failed one.
pub fn run_tool(dir: &Path, args: &[&str]) -> Result<ToolRun, String> {
    let extra = Extra {
        tool: true,
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    Ok(ToolRun {
        code: out.code,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Runs git in `dir` (on this machine) and returns its exit code and output: stdout when it
/// succeeds, stderr when it doesn't. For a caller that reads failures as answers (see
/// [`super::overview`]), which runs a repository elsewhere through one script instead.
pub(super) fn run_coded(dir: &Path, args: &[&str]) -> Result<(i32, Vec<u8>), String> {
    let out = execute(Some(dir), &Extra::default(), args)?;
    match out.code {
        Some(0) => Ok((0, out.stdout)),
        code => Ok((code.unwrap_or(-1), out.stderr)),
    }
}

/// What a [`run_call`] is run with besides its arguments.
#[derive(Default)]
pub struct Call {
    /// Variables set for it (`GIT_INDEX_FILE`…).
    pub env: Vec<(&'static str, String)>,
    /// What it reads on its input.
    pub stdin: Option<Vec<u8>>,
    /// It may take minutes (see `remote::Request::long`).
    pub long: bool,
}

/// Runs git with `args` in `dir` as `call` says, and returns its output as raw bytes, or the
/// error output.
pub fn run_call(dir: Option<&Path>, args: &[&str], call: Call) -> Result<Vec<u8>, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: call.env,
        stdin: call.stdin,
        long: call.long,
        ..Extra::default()
    };
    let out = execute(dir, &extra, args)?;
    if out.code == Some(0) {
        return Ok(out.stdout);
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(if said.is_empty() {
        crate::i18n::message(
            "backend-git-failed-da095f",
            &[
                ("args", (args.join(" ")).to_string().into()),
                ("status", (out.status).to_string().into()),
            ],
        )
    } else {
        said
    })
}

/// How a git call ended: its exit code (`None`: ended by a signal) and what it printed.
#[derive(Clone, Debug)]
pub struct Exit {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(crate) fn run_for_test_input(
    dir: &Path,
    args: &[&str],
    limit: std::time::Duration,
    stdin: Option<Vec<u8>>,
) -> Result<Exit, String> {
    let extra = Extra {
        test: Some(limit),
        stdin,
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    Ok(Exit {
        code: out.code,
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Immutable actual-machine authority for one durable workspace operation.
pub(crate) struct WorkspaceCommand<'a> {
    pub authority: &'a remote::workspace_authority::WorkspaceAuthority,
    pub input: Option<Vec<u8>>,
    pub cleanup: bool,
    pub directory: Option<&'a Path>,
}
impl<'a> WorkspaceCommand<'a> {
    pub fn new(authority: &'a remote::workspace_authority::WorkspaceAuthority) -> Self {
        Self {
            authority,
            input: None,
            cleanup: false,
            directory: None,
        }
    }
}
pub(crate) fn run_owned_test(
    dir: &Path,
    args: &[&str],
    record: &Path,
    limit: std::time::Duration,
    cancel: crate::tasks::Cancellation,
    authority: &remote::workspace_authority::WorkspaceAuthority,
) -> Result<crate::processes::Output, String> {
    run_workspace_command(
        dir,
        args,
        record,
        limit,
        cancel,
        WorkspaceCommand::new(authority),
    )
}
pub(crate) fn run_workspace_command(
    dir: &Path,
    args: &[&str],
    record: &Path,
    limit: std::time::Duration,
    cancel: crate::tasks::Cancellation,
    owned: WorkspaceCommand<'_>,
) -> Result<crate::processes::Output, String> {
    let extra = Extra {
        test: Some(limit),
        stdin: owned.input.clone(),
        ..Default::default()
    };
    let mut options = crate::processes::Options {
        cancel,
        background: true,
        timeout: Some(limit),
        ..Default::default()
    };
    let command = if let Some(target) = remote::parse(dir) {
        let record = remote::parse(record).ok_or(crate::i18n::text(
            "backend-remote-owner-record-is-on-a-different-machine-5eab4e",
        ))?;
        if record.host != target.host {
            return Err(crate::i18n::text(
                "backend-remote-owner-record-is-on-a-different-machine-5eab4e",
            )
            .into());
        }
        if owned.directory.is_some() {
            return Err(crate::i18n::text(
                "backend-remote-private-directories-use-their-fenced-shell-operation-9bbfda",
            )
            .into());
        }
        let request = remote_request(&target, &extra, args);
        remote::streaming_command_authority(
            &target.host,
            &request,
            Some(&record.path),
            Some(owned.authority),
            owned.cleanup,
        )?
        .configure(&mut options)
    } else {
        if remote::parse(record).is_some() {
            return Err(crate::i18n::text(
                "backend-native-owner-record-is-on-a-different-machine-56f2b5",
            )
            .into());
        }
        remote::native_owner::command_with_authority(
            local_command(Some(dir), &extra, args),
            record,
            limit,
            Some(owned.authority.clone()),
        )?
        .with_input(owned.input.as_deref(), owned.cleanup)?
        .with_directory(owned.directory)?
        .configure(&mut options)
    };
    crate::processes::run(command, options)
}

/// Like [`run`], but exit codes in `ok` count as success (`diff --no-index` exits 1 on changes).
pub fn run_ok_codes(dir: Option<&Path>, args: &[&str], ok: &[i32]) -> Result<String, String> {
    run_inner(dir, args, ok, None, false)
}

/// [`run_with`] for an operation someone asked for: when one of its hooks makes it fail (a
/// commit's `pre-commit`, a push's `pre-push`…), the error names that hook first.
pub fn run_naming_hooks(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
) -> Result<String, String> {
    run_extra(
        dir,
        args,
        &[0],
        creds,
        Extra {
            trace_hooks: super::hooks::wants_trace(args),
            ..Extra::default()
        },
    )
}

/// A username and password (or token) for an HTTPS remote, typed into gitcraft.
#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    /// None is legacy/unbound state and is refused by all transient helpers.
    pub scope: Option<super::CredentialScope>,
}

/// [`run`], answering a credential request with `creds` when given.
pub fn run_with(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
) -> Result<String, String> {
    run_inner(dir, args, &[0], creds, false)
}

/// [`run_with`] for an operation nobody is waiting on (the background fetch): it asks nobody
/// for credentials (see [`unattended`]). Helpers that answer from what they stored still do,
/// as do credentials typed into the tab (`creds`).
pub fn run_unattended(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
) -> Result<String, String> {
    run_inner(dir, args, &[0], creds, true)
}

/// [`run_unattended`] (with no credentials typed in) that, on another machine, is stopped there
/// after `limit`: for what runs for many repositories at once, which gives up on each after a
/// while and must not leave it running.
pub fn run_unattended_within(
    dir: &Path,
    args: &[&str],
    limit: std::time::Duration,
) -> Result<String, String> {
    run_inner_extra(Some(dir), args, &[0], None, true, Some(limit))
}

/// What sets git (before its subcommand) to open no sign-in window: no credential helper window
/// (Git Credential Manager and git ≥ 2.46 heed `credential.interactive`), nor git's askpass
/// program. git reads `GIT_ASKPASS` before `core.askPass` and `SSH_ASKPASS`, so an inherited one
/// (VS Code's terminal, ksshaskpass) would win over an empty `core.askPass`, and its being set
/// but empty makes git skip all three. ssh asks on the terminal the app was started from, if
/// any (a passphrase, a new host key: the fetch would wait there for good), or else runs
/// `SSH_ASKPASS` itself: on Unix (`unix`: where git runs) it is made to ask a program that
/// answers nothing, `false`, and nothing else; where that isn't there, never to run one.
fn unattended_extra(unix: bool) -> Extra {
    let mut env = vec![("GIT_ASKPASS", String::new())];
    if unix {
        env.push(("SSH_ASKPASS_REQUIRE", "force".to_owned()));
        env.push(("SSH_ASKPASS", "false".to_owned()));
    } else {
        env.push(("SSH_ASKPASS_REQUIRE", "never".to_owned()));
    }
    Extra {
        pre: vec!["-c", "credential.interactive=false", "-c", "core.askPass="],
        env,
        ..Extra::default()
    }
}

/// Sets `cmd` to open no sign-in window (see [`unattended_extra`]).
fn unattended(cmd: &mut Command) {
    let extra = unattended_extra(cfg!(unix));
    cmd.args(&extra.pre).args(&extra.owned_pre);
    for (name, value) in extra.env {
        cmd.env(name, value);
    }
}

/// Whether `err` says git needed a username / password it couldn't get or that were refused.
pub fn is_auth_error(err: &str) -> bool {
    let e = err.to_lowercase();
    [
        "terminal prompts disabled",
        "could not read username",
        "could not read password",
        "authentication failed",
        "invalid username or password",
        "http basic: access denied",
        "the requested url returned error: 401",
        "the requested url returned error: 403",
    ]
    .iter()
    .any(|s| e.contains(s))
}

/// What a call is run with on top of [`GIT_ENV`]: nobody asked to sign in when `quiet`,
/// `creds` given to git when there are some.
fn extra_for(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
) -> Result<Extra, String> {
    let mut extra = if quiet {
        unattended_extra(cfg!(unix) || dir.is_some_and(remote::is_remote))
    } else {
        Extra::default()
    };
    if let Some(c) = creds {
        extra.normalize_repository = true;
        let policy = credential_command_policy(dir, c, args)?;
        extra.owned_pre.extend(policy.arguments);
        extra.env.extend(policy.environment);
    }
    Ok(extra)
}

fn run_inner_extra(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
    timeout: Option<std::time::Duration>,
) -> Result<String, String> {
    let mut extra = extra_for(dir, args, creds, quiet)?;
    extra.timeout = timeout;
    finish(execute(dir, &extra, args)?, args, ok)
}

fn run_extra(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    mut extra: Extra,
) -> Result<String, String> {
    let credentials = extra_for(dir, args, creds, false)?;
    extra.pre.extend(credentials.pre);
    extra.owned_pre.extend(credentials.owned_pre);
    extra.normalize_repository |= credentials.normalize_repository;
    extra.env.extend(credentials.env);
    finish(execute(dir, &extra, args)?, args, ok)
}

/// The error of a git call that failed: what it said, or how it ended when it said nothing.
fn failure_message(args: &[&str], status: &str, stdout: &str, stderr: &str) -> String {
    // Merges and pulls report conflicts on stdout and the summary on stderr.
    let (stderr, stdout) = (stderr.trim(), stdout.trim());
    match (stderr.is_empty(), stdout.is_empty()) {
        (true, true) => crate::i18n::message(
            "backend-git-failed-status-db85b2",
            &[
                ("args", (args.join(" ")).to_string().into()),
                ("status", (status).to_string().into()),
            ],
        ),
        (false, true) => stderr.to_owned(),
        (true, false) => stdout.to_owned(),
        (false, false) => format!("{stderr}\n{stdout}"),
    }
}

fn run_inner(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
) -> Result<String, String> {
    let extra = extra_for(dir, args, creds, quiet)?;
    finish(execute(dir, &extra, args)?, args, ok)
}

/// [`run`] with `GIT_INDEX_FILE` set to `index`, a path on the machine git runs on: for work
/// on a copy of the index (or a new one) that leaves the repository's own index alone.
pub fn run_with_index(dir: &Path, index: &str, args: &[&str]) -> Result<String, String> {
    let extra = Extra {
        env: vec![("GIT_INDEX_FILE", index.to_owned())],
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    outcome(out, args, &[0])
}

/// Stdout when git exited with one of the codes `ok`, else what it said.
fn outcome(out: Ran, args: &[&str], ok: &[i32]) -> Result<String, String> {
    finish(out, args, ok)
}

/// [`run`] with variables for git on top of the usual ones, here or on the machine git runs on
/// for `dir`: a scratch index (`GIT_INDEX_FILE`), the author of a commit being made.
pub fn run_env_owned(
    dir: &Path,
    env: &[(&'static str, String)],
    args: &[&str],
) -> Result<String, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: env.to_vec(),
        ..Extra::default()
    };
    finish(execute(Some(dir), &extra, args)?, args, &[0])
}

/// What a call that ended as `out` gives: its output when its exit code is one of `ok`, else
/// what it said went wrong.
fn finish(out: Ran, args: &[&str], ok: &[i32]) -> Result<String, String> {
    // For the doctor: what hooks print is lost once the command succeeds.
    let succeeded = out.code.is_some_and(|c| ok.contains(&c));
    if succeeded {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        // Merges and pulls report conflicts on stdout and the summary on stderr.
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let said = match (stderr.is_empty(), stdout.is_empty()) {
            (true, true) => String::new(),
            (false, true) => stderr,
            (true, false) => stdout,
            (false, false) => format!("{stderr}\n{stdout}"),
        };
        Err(download_read_error(
            args,
            match (&out.failed_hook, said.is_empty()) {
                (Some(hook), _) => hook.explain(&said, args),
                (None, true) => crate::i18n::message(
                    "backend-git-failed-da095f",
                    &[
                        ("args", (args.join(" ")).to_string().into()),
                        ("status", (out.status).to_string().into()),
                    ],
                ),
                (None, false) => said,
            },
        ))
    }
}

/// [`run_inner`], and what the call did, for the activity log.
fn run_inner_recorded(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
) -> (Result<String, String>, Recorded) {
    let started = std::time::Instant::now();
    let mut extra = match extra_for(dir, args, creds, quiet) {
        Ok(extra) => extra,
        Err(e) => {
            return (
                Err(e.clone()),
                Recorded::new(args, None, &e, creds, started),
            );
        }
    };
    extra.trace_hooks = !quiet && super::hooks::wants_trace(args);
    let out = match execute(dir, &extra, args) {
        Ok(out) => out,
        Err(e) => {
            return (
                Err(e.clone()),
                Recorded::new(args, None, &e, creds, started),
            );
        }
    };
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let recorded = Recorded::new(args, out.code, &both(&stdout, &stderr), creds, started);
    let result = finish(out, args, ok);
    (result, recorded)
}

/// What a command printed, its output then its error output.
fn both(stdout: &str, stderr: &str) -> String {
    match (stdout.trim_end(), stderr.trim_end()) {
        ("", err) => err.to_owned(),
        (out, "") => out.to_owned(),
        (out, err) => format!("{out}\n{err}"),
    }
}

/// What one git call did, for the activity log: its command line, how it ended, what it
/// printed and how long it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// The arguments after `git`, as the caller gave them (credentials never are: they go to
    /// git through the environment).
    pub args: Vec<String>,
    /// Its exit code; `None` when it couldn't run, or was stopped.
    pub code: Option<i32>,
    /// What it printed (output, then error output; progress lines as last drawn).
    pub output: String,
    pub took: std::time::Duration,
}

impl Recorded {
    /// What a call did, its secrets hidden (see [`super::redact`]): the password typed into
    /// the tab (`creds`) among them.
    pub(crate) fn new(
        args: &[&str],
        code: Option<i32>,
        output: &str,
        creds: Option<&Credentials>,
        started: std::time::Instant,
    ) -> Self {
        crate::diagnostics::crash::operation(args, code, started.elapsed());
        let password = creds.map(|c| c.password.as_str());
        Self {
            args: args.iter().map(|a| super::redact(a, password)).collect(),
            code,
            output: super::redact(output, password),
            took: started.elapsed(),
        }
    }
}

/// [`run_with`] (or [`run_unattended`] when `quiet`), and what the call did.
pub fn run_recorded(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
) -> (Result<String, String>, Recorded) {
    run_inner_recorded(dir, args, &[0], creds, quiet)
}

/// What git's credential helpers keep for `url` (`https://host`), asking nobody: no terminal
/// prompt, no askpass program, no sign-in window.
pub fn stored_credentials(url: &str) -> Option<Credentials> {
    let _timer = super::backend::CliTimer::new(&["credential"]);
    use std::io::Write;
    use std::process::Stdio;
    let mut cmd = git_cmd();
    with_profile(&mut cmd, &["credential", "fill"]);
    unattended(&mut cmd);
    cmd.args(["credential", "fill"])
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("url={url}\n\n").as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    let found = parse_credentials(&String::from_utf8_lossy(&out.stdout))?;
    Credentials::for_url(url, found.username, found.password).ok()
}

/// The name and password in what `git credential fill` prints.
fn parse_credentials(out: &str) -> Option<Credentials> {
    let field = |name: &str| {
        out.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(str::to_owned)
    };
    let (username, password) = (field("username")?, field("password")?);
    (!password.is_empty()).then_some(Credentials {
        username,
        password,
        scope: None,
    })
}

/// Bounded, non-fetching preparation for opt-in local syntax comparison.
pub(crate) fn syntax_read(
    repo: &Path,
    args: &[&str],
    limit: usize,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    if crate::remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-syntax-comparison-is-currently-available-for-local-repositories-s-c279dd",
        )
        .to_owned());
    }
    let args: Vec<_> = std::iter::once("--no-lazy-fetch")
        .chain(args.iter().copied())
        .collect();
    bounded::local(repo, &args, Vec::new(), limit, timeout).map_err(syntax_read_error)
}

fn syntax_read_error(error: String) -> String {
    if error
        .lines()
        .any(|line| line.trim() == "unknown option: --no-lazy-fetch")
    {
        crate::i18n::message(
            "backend-syntax-preparation-requires-git-with-no-lazy-fetch-support-error-0c471c",
            &[("detail", (error).to_string().into())],
        )
    } else {
        error
    }
}

/// Bounded raw snapshots: no implicit network fetches, filters or detached workers.
pub(super) fn comparison_output(
    repo: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Comparison cancelled".into());
    }
    let guarded: Vec<_> = std::iter::once("--no-lazy-fetch")
        .chain(args.iter().copied())
        .collect();
    let out = if let Some(target) = remote::parse(repo) {
        let mut request = bounded::request(&target, &guarded, Vec::new(), 8 * 1024 * 1024);
        request
            .env
            .extend(env.iter().map(|(k, v)| ((*k).into(), v.clone())));
        comparison_remote(&target.host, request, active)?
    } else {
        let extra = Extra {
            env: env.to_vec(),
            ..Default::default()
        };
        let cmd = local_command(Some(repo), &extra, &guarded);
        let out = crate::processes::run_while(
            cmd,
            crate::processes::Options {
                timeout: Some(std::time::Duration::from_secs(20)),
                output_limit: 8 * 1024 * 1024,
                stderr_limit: Some(64 * 1024),
                ..Default::default()
            },
            active,
        )?
        .complete()?;
        remote::Output {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        }
    };
    if out.stdout.len() > 8 * 1024 * 1024 || out.stderr.len() > 64 * 1024 {
        return Err(crate::i18n::text(
            "backend-comparison-output-exceeds-the-size-limit-choose-fewer-files-727a73",
        )
        .into());
    }
    if !active() {
        return Err("Comparison cancelled".into());
    }
    Ok(out)
}
pub(super) fn comparison_remote(
    host: &remote::Host,
    mut request: remote::Request,
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Comparison cancelled".into());
    }
    request.timeout = Some(std::time::Duration::from_secs(20));
    request.long = true;
    let out = remote::exec(host, &request)?;
    if !active() {
        return Err("Comparison cancelled".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn syntax_errors_only_suggest_upgrade_for_the_unknown_git_option() {
        for error in [
            "cancelled",
            "process timed out and was stopped",
            "fatal: bad object",
            "process exceeded capture limit",
        ] {
            assert_eq!(super::syntax_read_error(error.into()), error);
        }
        let unsupported = "unknown option: --no-lazy-fetch\nusage: git [...]";
        assert!(
            super::syntax_read_error(unsupported.into())
                .contains("requires Git with --no-lazy-fetch support")
        );
        let incidental = "fatal: file named unknown option: --no-lazy-fetch";
        assert_eq!(super::syntax_read_error(incidental.into()), incidental);
    }
    use std::io::Write;
    #[cfg(unix)]
    use std::process::Stdio;

    use super::*;

    #[test]
    fn large_binary_reads_and_file_exports_exceed_the_diagnostic_capture_bound()
    -> Result<(), String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-large-output-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        run(Some(&dir), &["init", "-q"])?;
        let blob = dir.join("large.bin");
        let mut file = std::fs::File::create(&blob).map_err(|e| e.to_string())?;
        let chunk = vec![0; 1 << 20];
        for _ in 0..65 {
            file.write_all(&chunk).map_err(|e| e.to_string())?;
        }
        drop(file);
        let hash = run(Some(&dir), &["hash-object", "-w", "large.bin"])?;
        let args = ["cat-file", "blob", hash.trim()];
        let captured = run_bytes(Some(&dir), &args)?.len();
        let export = dir.join("export.bin");
        run_to_file(&dir, &args, &export)?;
        let saved = std::fs::metadata(&export).map_err(|e| e.to_string())?.len();
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(captured, 65 << 20);
        assert_eq!(saved, captured as u64);
        Ok(())
    }

    /// Network operations go to shells of their own on another machine; reads don't.
    #[test]
    fn network_operations_are_told_apart() {
        for (args, long) in [
            (&["fetch", "--prune"][..], true),
            (&["-c", "credential.helper=x", "push", "origin"][..], true),
            (&["--literal-pathspecs", "pull"][..], true),
            (&["submodule", "update"][..], true),
            (
                &[
                    "-c",
                    "alias.gitcraft-user-command=!cargo test",
                    USER_COMMAND_ALIAS,
                ][..],
                true,
            ),
            (&["status", "--porcelain"][..], false),
            (&["-c", "fetch.x=1", "log"][..], false),
            (&["--no-optional-locks", "status"][..], false),
            (&[][..], false),
        ] {
            assert_eq!(goes_over_network(args), long, "{args:?}");
        }
    }

    /// A diff or merge tool on another machine stays open as long as the user works in it, in
    /// a shell of its own; any other read keeps its lane's limit.
    #[test]
    fn a_tool_on_another_machine_may_stay_open_for_hours() {
        let target = Target {
            host: remote::Host::Ssh("build".to_owned()),
            path: "/r".to_owned(),
        };
        let tool = Extra {
            tool: true,
            ..Extra::default()
        };
        let req = remote_request(&target, &tool, &["mergetool", "--tool=meld", "-y"]);
        assert!(req.long);
        assert_eq!(req.timeout, Some(TOOL_FOR));
        assert!(
            req.script
                .starts_with("exec git '--no-lazy-fetch' 'mergetool'"),
            "{}",
            req.script
        );
        let read = remote_request(&target, &Extra::default(), &["status"]);
        assert!(!read.long);
        assert_eq!(read.timeout, None);
    }

    /// `git config --global` reaches the user's own file, whatever comes before it; every
    /// other command (and a repository's own `config`) goes by the profile.
    #[test]
    fn only_global_config_commands_skip_the_profile() {
        assert!(user_global_config(&["config", "--global", "user.name"]));
        assert!(user_global_config(&[
            "-c", "core.x=y", "config", "--global", "--list"
        ]));
        assert!(!user_global_config(&["config", "user.name"]));
        assert!(!user_global_config(&["commit", "-m", "--global"]));
        assert!(!user_global_config(&["-c", "config"]));
        // Whatever options come before the subcommand.
        assert!(user_global_config(&["-C", "/r", "config", "--global", "x"]));
        assert!(user_global_config(&[
            "--literal-pathspecs",
            "config",
            "--global",
            "x"
        ]));
        assert!(user_global_config(&["-ck=v", "config", "--global", "x"]));
        assert!(user_global_config(&["maintenance", "start"]));
        // Running it reads the configuration as every command does.
        assert!(!user_global_config(&["maintenance", "run"]));
        assert!(!user_global_config(&[]));
    }

    /// What git gets from the helper is what was typed, backslashes (`DOMAIN\user`) included.
    #[test]
    fn credentials_preserve_typed_fields_without_authorizing_helper_only_use() -> Result<(), String>
    {
        let (user, pass) = ("CORP\\nancy", "p\\c%s$x 'q\"");
        let credentials =
            Credentials::for_url("https://example.invalid/", user.into(), pass.into())?;
        let environment = credentials.helper_environment()?;
        assert!(environment.contains(&("GITCRAFT_GIT_USERNAME", user.into())));
        assert!(environment.contains(&("GITCRAFT_GIT_PASSWORD", pass.into())));
        assert!(
            !environment
                .iter()
                .any(|(key, _)| *key == "GITCRAFT_CREDENTIAL_OWNER")
        );
        Ok(())
    }

    #[cfg(unix)]
    fn scratch(name: &str) -> Result<std::path::PathBuf, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-cmd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    /// A script that leaves `marker` behind when run, then prints `x` (an askpass answer).
    #[cfg(unix)]
    fn marking_script(dir: &Path, marker: &Path) -> Result<std::path::PathBuf, String> {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("askpass.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\necho x\n", marker.display()),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(script)
    }

    /// Whether git, asked for a credential (quietly or not) in an environment that sets
    /// `SSH_ASKPASS` (KDE's opens a window) and maybe `GIT_ASKPASS` (VS Code's terminal), ran
    /// the askpass program.
    #[cfg(unix)]
    fn askpass_asked(dir: &Path, quiet: bool, git_askpass: bool) -> Result<bool, String> {
        let marker = dir.join("asked");
        let _ = std::fs::remove_file(&marker);
        let script = marking_script(dir, &marker)?;
        let mut cmd = git_cmd();
        // Not the user's own config, whose `core.askPass` would stand in for the script.
        cmd.env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        // What the app inherited, which `unattended` then has to override.
        cmd.env("SSH_ASKPASS", &script);
        if git_askpass {
            cmd.env("GIT_ASKPASS", &script);
        } else {
            cmd.env_remove("GIT_ASKPASS");
        }
        if quiet {
            unattended(&mut cmd);
        }
        let mut child = cmd
            .args(["-c", "credential.helper=", "credential", "fill"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"protocol=https\nhost=example.invalid\n\n");
        }
        child.wait().map_err(|e| e.to_string())?;
        Ok(marker.exists())
    }

    /// The background fetch asks nobody to sign in, every few minutes, through a window: not
    /// even with a `GIT_ASKPASS` in the environment, which git reads before `core.askPass`.
    #[cfg(unix)]
    #[test]
    fn a_quiet_operation_opens_no_sign_in_window() -> Result<(), String> {
        let dir = scratch("unattended")?;
        let asked = [(false, false), (true, false), (false, true), (true, true)]
            .map(|(quiet, git_askpass)| askpass_asked(&dir, quiet, git_askpass));
        let _ = std::fs::remove_dir_all(&dir);
        // Without it, git does ask (or the test would prove nothing).
        assert_eq!(asked, [Ok(true), Ok(false), Ok(true), Ok(false)]);
        Ok(())
    }

    /// Nor does ssh, which would ask on a terminal or run `SSH_ASKPASS` itself.
    #[cfg(unix)]
    #[test]
    fn a_quiet_operation_tells_ssh_to_ask_nobody() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("unattended-ssh")?;
        let seen = dir.join("seen");
        let ssh = dir.join("ssh.sh");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\nprintf '%s %s' \"$SSH_ASKPASS_REQUIRE\" \"$SSH_ASKPASS\" > '{}'\nexit 1\n",
                seen.display()
            ),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let mut cmd = git_cmd();
        cmd.env("GIT_SSH_COMMAND", &ssh)
            .env_remove("SSH_ASKPASS_REQUIRE");
        unattended(&mut cmd);
        let out = cmd
            .args(["ls-remote", "ssh://example.invalid/repo.git"])
            .output()
            .map_err(|e| e.to_string())?;
        let required = std::fs::read_to_string(&seen);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!out.status.success());
        // Asked through `false`, which answers nothing, never on a terminal.
        assert_eq!(required.map_err(|e| e.to_string())?, "force false");
        Ok(())
    }

    /// Credentials typed into the tab still answer a quiet operation.
    #[cfg(unix)]
    #[test]
    fn quiet_transient_credentials_use_the_shared_transport_policy() -> Result<(), String> {
        let credentials = Credentials::for_url(
            "https://example.invalid/",
            "nancy".into(),
            "DUMMY_SECRET".into(),
        )?;
        let extra = extra_for(None, &["fetch", "origin"], Some(&credentials), true)?;
        assert!(extra.normalize_repository);
        assert_eq!(
            extra.owned_pre.last().map(String::as_str),
            Some("gitcraft-credential-transport")
        );
        assert!(
            extra
                .env
                .contains(&("GITCRAFT_GIT_PASSWORD", "DUMMY_SECRET".into()))
        );
        assert!(!extra.owned_pre.join(" ").contains("DUMMY_SECRET"));
        assert!(
            !extra
                .env
                .iter()
                .any(|(key, _)| *key == "GITCRAFT_CREDENTIAL_OWNER")
        );
        Ok(())
    }

    #[test]
    fn reads_a_stored_credential() {
        let out = "protocol=https\nhost=codeberg.org\nusername=ana\npassword=s3=cret\n";
        let creds = parse_credentials(out);
        assert_eq!(
            creds.map(|c| (c.username, c.password)),
            Some(("ana".to_owned(), "s3=cret".to_owned()))
        );
        assert!(parse_credentials("protocol=https\nhost=x\n").is_none());
        assert!(parse_credentials("username=ana\npassword=\n").is_none());
    }
}

/// Bounded isolated object reads/replays on the shared process service. Task cancellation
/// owns both local children and remote completion; no detached transport is introduced.
pub(crate) fn forecast_bytes(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    draft_bytes(dir, args, env, Vec::new(), ok, active)
}

pub(crate) fn draft_bytes(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    input: Vec<u8>,
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if !active() || crate::tasks::cancellation().is_cancelled() {
        return Err("Read cancelled".into());
    }
    let extra = Extra {
        env: [("GIT_NO_REPLACE_OBJECTS", "1".into())]
            .into_iter()
            .chain(env.iter().cloned())
            .collect(),
        stdin: Some(input),
        long: true,
        timeout: Some(std::time::Duration::from_secs(15)),
        output_limit: Some(64 << 20),
        ..Default::default()
    };
    const REMOVE: &[&str] = &[
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_ATTR_SOURCE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ];
    let (code, stdout, stderr) = if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, &extra, args);
        request.unset.extend(REMOVE.iter().map(|s| (*s).to_owned()));
        let out = remote::exec(&target.host, &request)?;
        (out.code, out.stdout, out.stderr)
    } else {
        let mut command = local_command(Some(dir), &extra, args);
        for key in REMOVE {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        let out = crate::processes::run(
            command,
            crate::processes::Options {
                input: extra.stdin,
                background: true,
                timeout: extra.timeout,
                output_limit: 64 << 20,
                stderr_limit: Some(64 << 10),
                ..Default::default()
            },
        )?
        .complete()?;
        (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
    };
    if !active() {
        return Err("Read cancelled".into());
    }
    if stdout.len() > 64 << 20 || stderr.len() > 64 << 10 {
        return Err(crate::i18n::text("backend-read-output-exceeds-limit-042a31").into());
    }
    if ok.contains(&code) && stderr.is_empty() {
        Ok(stdout)
    } else {
        Err(crate::i18n::message(
            "backend-git-command-failed-code-a5e2b6",
            &[
                ("code", (code).to_string().into()),
                (
                    "from_utf8_lossy",
                    (String::from_utf8_lossy(&stderr).trim()).to_string().into(),
                ),
            ],
        ))
    }
}

pub(crate) fn forecast_run(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    forecast_bytes(dir, args, env, ok, active).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub(crate) fn forecast_remote(
    host: &remote::Host,
    mut request: remote::Request,
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Read cancelled".into());
    }
    request.timeout = Some(std::time::Duration::from_secs(15));
    request.long = true;
    remote::exec(host, &request)
}

/// Raw object recovery transport with bounded output and current task ownership. It never
/// abandons a remote call while that call still owns a session or remote process.
pub(super) fn recovery_output(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
    timeout: std::time::Duration,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Recovery cancelled".into());
    }
    const MAX: usize = 16 << 20;
    const REMOVE: &[&str] = &[
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_ATTR_SOURCE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_SHALLOW_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_SUPER_PREFIX",
        "GIT_LITERAL_PATHSPECS",
        "GIT_GLOB_PATHSPECS",
        "GIT_NOGLOB_PATHSPECS",
        "GIT_ICASE_PATHSPECS",
    ];
    let extra = Extra {
        stdin: input.map(<[u8]>::to_vec),
        timeout: Some(timeout),
        long: true,
        ..Default::default()
    };
    let out = if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, &extra, args);
        request.unset.extend(REMOVE.iter().map(|s| (*s).into()));
        let command = request
            .script
            .strip_prefix("exec ")
            .unwrap_or(&request.script)
            .to_owned();
        request.script = format!(
            "umask 077\nrecovery_io=$(mktemp -d) || exit 1\ntrap 'rm -rf -- \"$recovery_io\"' 0\n\
             mkfifo \"$recovery_io/out\" \"$recovery_io/err\" || exit 1\n\
             head -c {cap} <\"$recovery_io/out\" >\"$recovery_io/out-cap\" & recovery_stdout=$!\n\
             head -c {cap} <\"$recovery_io/err\" >\"$recovery_io/err-cap\" & recovery_stderr=$!\n\
             {command} >\"$recovery_io/out\" 2>\"$recovery_io/err\"; recovery_code=$?\n\
             wait \"$recovery_stdout\"; wait \"$recovery_stderr\"\n\
             cat \"$recovery_io/out-cap\"; cat \"$recovery_io/err-cap\" >&2\nexit \"$recovery_code\"",
            cap = MAX + 1
        );
        remote::exec(&target.host, &request)?
    } else {
        let mut command = local_command(Some(dir), &extra, args);
        for key in REMOVE {
            command.env_remove(key);
        }
        let out = crate::processes::run(
            command,
            crate::processes::Options {
                input: extra.stdin,
                background: true,
                timeout: Some(timeout),
                output_limit: MAX,
                stderr_limit: Some(MAX),
                ..Default::default()
            },
        )?
        .complete()?;
        remote::Output {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        }
    };
    if !active() {
        return Err("Recovery cancelled".into());
    }
    if out.stdout.len() > MAX || out.stderr.len() > MAX {
        return Err(crate::i18n::text("backend-recovery-output-exceeds-limit-3e73fc").into());
    }
    Ok(out)
}

pub(crate) struct PushTarget<'a> {
    pub address: &'a str,
    pub skip_hooks: bool,
    pub alias: &'a str,
    pub original_remote: &'a str,
    pub lfs_config: &'a [(String, String)],
    pub lfs_objects: &'a [String],
    pub receive_pack: &'a str,
}

pub(crate) fn push_preflight_with(
    dir: &Path,
    args: &[String],
    target: PushTarget<'_>,
    creds: Option<&Credentials>,
    control: Option<&std::sync::Arc<OpControl>>,
) -> Result<String, String> {
    let PushTarget {
        address,
        skip_hooks,
        alias: opaque,
        original_remote,
        lfs_config,
        lfs_objects,
        receive_pack,
    } = target;
    let (source, destination) =
        args.last()
            .and_then(|s| s.split_once(':'))
            .ok_or(crate::i18n::text(
                "backend-missing-inspected-refspec-fa73fb",
            ))?;
    let remote_tip = args.iter().find_map(|a| {
        a.strip_prefix("--force-with-lease=")
            .and_then(|v| v.rsplit_once(':'))
            .map(|v| v.1)
    });
    let hooks = super::push_hooks::Hooks::new(
        dir,
        original_remote,
        super::push_hooks::Upload {
            alias: opaque,
            skip_original: skip_hooks,
            objects: lfs_objects,
            source,
            destination,
            remote_tip,
        },
    )?;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut extra = reviewed_push_extra(
        dir,
        &words,
        PushTarget {
            address,
            skip_hooks,
            alias: opaque,
            original_remote,
            lfs_config,
            lfs_objects,
            receive_pack,
        },
        hooks.path(),
        creds,
    )?;
    let mapping: Vec<_> = extra
        .env
        .iter()
        .filter(|(name, _)| name.starts_with("GIT_CONFIG_"))
        .cloned()
        .collect();
    if forecast_run(
        dir,
        // get-url requires a locally configured remote. The original and private
        // alias now have the same single explicit URL/pushurl (empty entries
        // clear inherited multi-address values). Prove push expansion using the
        // configured original, while transport/hooks use the private alias.
        &["remote", "get-url", "--push", "--all", original_remote],
        &mapping,
        &[0],
        &|| true,
    )
    .map_err(|_| {
        crate::i18n::text("backend-the-reviewed-address-could-not-be-pinned-no-push-was-run-8a62d8")
    })?
    .trim_end()
        != address
    {
        return Err(crate::i18n::text(
            "backend-the-reviewed-address-could-not-be-pinned-no-push-was-run-8a62d8",
        )
        .into());
    }
    // Preserve only Git's structured local hook identity/status. Arbitrary hook and
    // server output remains private, as it can contain credentials or secrets.
    let trace = (!remote::is_remote(dir))
        .then(super::hooks::TraceFile::new)
        .flatten();
    if let Some(trace) = &trace {
        extra.env.push((
            super::hooks::TRACE_VAR,
            trace.path().to_string_lossy().into_owned(),
        ));
    }
    let out = execute_preflight(dir, &extra, &words, control)?;
    if out.code == Some(0) {
        Ok(crate::i18n::text("backend-push-completed-5fccd0").into())
    } else if let Some(hook) = trace.and_then(|trace| trace.failed_hook(&words)) {
        Err(hook.explain(
            "Hook output withheld because it can contain credentials or secrets.",
            &words,
        ))
    } else if is_auth_error(&String::from_utf8_lossy(&out.stderr)) {
        Err(
            crate::i18n::text("backend-authentication-failed-for-the-reviewed-push-target-75e906")
                .into(),
        )
    } else {
        Err(reviewed_refusal(&out.stdout,source,destination).unwrap_or_else(||crate::i18n::text("backend-the-reviewed-push-was-refused-or-failed-fetch-and-reopen-prefligh-59a6f0").into()))
    }
}

/// Pure construction shared by watched and ordinary reviewed-push runners.
fn reviewed_push_extra(
    dir: &Path,
    words: &[&str],
    target: PushTarget<'_>,
    hooks_path: String,
    creds: Option<&Credentials>,
) -> Result<Extra, String> {
    let PushTarget {
        address,
        alias: opaque,
        original_remote,
        lfs_config,
        receive_pack,
        ..
    } = target;
    let placeholder = format!("gitcraft-preflight-address://{opaque}");
    let mut values = vec![
        (format!("url.{address}.insteadOf"), placeholder.clone()),
        (format!("remote.{opaque}.url"), String::new()),
        (format!("remote.{opaque}.url"), placeholder.clone()),
        // An explicit pushurl disables pushInsteadOf for this private remote.
        // Otherwise a push-only rule for the placeholder prefix can bypass the
        // ordinary URL expansion proof while leaving its config stamp unchanged.
        (format!("remote.{opaque}.pushurl"), String::new()),
        (format!("remote.{opaque}.pushurl"), placeholder.clone()),
        (format!("remote.{original_remote}.url"), String::new()),
        (format!("remote.{original_remote}.url"), placeholder.clone()),
        (format!("remote.{original_remote}.pushurl"), String::new()),
        (format!("remote.{original_remote}.pushurl"), placeholder),
        ("core.hooksPath".into(), hooks_path),
    ];
    for (source, value) in lfs_config {
        if value.is_empty() {
            continue;
        }
        let target = source
            .strip_prefix(&format!("remote.{original_remote}."))
            .map(|key| format!("remote.{opaque}.{key}"))
            .unwrap_or_else(|| source.clone());
        values.push((target.clone(), value.clone()));
        if target != *source {
            values.push((source.clone(), value.clone()));
        }
    }
    if !receive_pack.is_empty() {
        values.push((format!("remote.{opaque}.receivepack"), receive_pack.into()));
    }
    const VARS: [(&str, &str); 16] = [
        ("GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0"),
        ("GIT_CONFIG_KEY_1", "GIT_CONFIG_VALUE_1"),
        ("GIT_CONFIG_KEY_2", "GIT_CONFIG_VALUE_2"),
        ("GIT_CONFIG_KEY_3", "GIT_CONFIG_VALUE_3"),
        ("GIT_CONFIG_KEY_4", "GIT_CONFIG_VALUE_4"),
        ("GIT_CONFIG_KEY_5", "GIT_CONFIG_VALUE_5"),
        ("GIT_CONFIG_KEY_6", "GIT_CONFIG_VALUE_6"),
        ("GIT_CONFIG_KEY_7", "GIT_CONFIG_VALUE_7"),
        ("GIT_CONFIG_KEY_8", "GIT_CONFIG_VALUE_8"),
        ("GIT_CONFIG_KEY_9", "GIT_CONFIG_VALUE_9"),
        ("GIT_CONFIG_KEY_10", "GIT_CONFIG_VALUE_10"),
        ("GIT_CONFIG_KEY_11", "GIT_CONFIG_VALUE_11"),
        ("GIT_CONFIG_KEY_12", "GIT_CONFIG_VALUE_12"),
        ("GIT_CONFIG_KEY_13", "GIT_CONFIG_VALUE_13"),
        ("GIT_CONFIG_KEY_14", "GIT_CONFIG_VALUE_14"),
        ("GIT_CONFIG_KEY_15", "GIT_CONFIG_VALUE_15"),
    ];
    if values.len() > VARS.len() {
        return Err(crate::i18n::text(
            "backend-too-many-pinned-transport-settings-no-push-was-run-f9ebc3",
        )
        .into());
    }
    let mut extra = Extra {
        pre: Vec::new(),
        unset: PREFLIGHT_REMOVE.to_vec(),
        env: vec![
            ("GIT_CONFIG_COUNT", values.len().to_string()),
            ("GIT_NO_REPLACE_OBJECTS", "1".into()),
        ],
        ..Default::default()
    };
    for ((key, value), (key_var, value_var)) in values.into_iter().zip(VARS) {
        extra.env.extend([(key_var, key), (value_var, value)]);
    }
    preflight_credentials(dir, &mut extra, words, address, creds)?;
    Ok(extra)
}

fn preflight_credentials(
    dir: &Path,
    extra: &mut Extra,
    words: &[&str],
    address: &str,
    creds: Option<&Credentials>,
) -> Result<(), String> {
    let Some(creds) = creds else {
        return Ok(());
    };
    // Validate the captured account even when this is another reviewed endpoint.
    // An unbound or edited cached secret must not acquire a new endpoint scope.
    creds.helper_environment()?;
    if creds
        .scope
        .as_ref()
        .is_some_and(|scope| scope.matches_url(address))
    {
        let policy = credential_command_policy(Some(dir), creds, words)?;
        extra.normalize_repository = true;
        extra.owned_pre.extend(policy.arguments);
        extra.env.extend(policy.environment);
    }
    Ok(())
}

const PREFLIGHT_REMOVE: &[&str] = &[
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_SHALLOW_FILE",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_ATTR_SOURCE",
];
fn execute_preflight(
    dir: &Path,
    extra: &Extra,
    words: &[&str],
    control: Option<&std::sync::Arc<OpControl>>,
) -> Result<Ran, String> {
    if let Some(control) = control {
        return watched::reviewed(dir, extra, words, control);
    }
    if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, extra, words);
        request
            .unset
            .extend(PREFLIGHT_REMOVE.iter().map(|s| (*s).to_owned()));
        let out = remote::exec(&target.host, &request)?;
        Ok(Ran {
            code: Some(out.code),
            status: out.code.to_string(),
            stdout: out.stdout,
            stderr: out.stderr,
            failed_hook: None,
        })
    } else {
        let cmd = local_command(Some(dir), extra, words);
        let out = crate::processes::run(
            cmd,
            crate::processes::Options {
                background: true,
                ..Default::default()
            },
        )?
        .complete()?;
        Ok(Ran {
            code: out.status.code(),
            status: out.status.to_string(),
            stdout: out.stdout,
            stderr: out.stderr,
            failed_hook: None,
        })
    }
}
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

fn reviewed_refusal(bytes: &[u8], source: &str, destination: &str) -> Option<String> {
    let out = std::str::from_utf8(bytes).ok()?;
    for line in out.lines() {
        let mut fields = line.splitn(3, '\t');
        if fields.next() != Some("!") {
            continue;
        }
        let refs = fields.next()?;
        let status = fields.next()?;
        if refs != format!("{source}:{destination}") {
            continue;
        }
        let safe = match status {
            "[rejected] (non-fast-forward)"
            | "[rejected] (fetch first)"
            | "[rejected] (stale info)"
            | "[rejected] (remote ref updated since checkout)" => status,
            s if s.starts_with("[remote rejected]") => {
                "[remote rejected] (remote policy refused the update; details withheld)"
            }
            _ => "[rejected] (update refused; details withheld)",
        };
        return Some(format!("!\t{source}:{destination}\t{safe}"));
    }
    None
}
#[cfg(test)]
mod reviewed_push_tests {
    use super::*;
    /// Production builder only: selected programs are fixed strings and never run.
    #[cfg(windows)]
    #[test]
    fn recording_reviewed_push_policy_preserves_counted_alias_and_exact_subcommand()
    -> Result<(), String> {
        let url = "https://fixture.invalid/team/repo.git";
        let creds = Credentials::for_url(url, "alice".into(), "DUMMY_ONLY".into())?;
        let result = reviewed_push_extra(
            Path::new("C:/recorded/repo"),
            &["push"],
            PushTarget {
                address: url,
                alias: "gitcraft-preflight-fixture",
                original_remote: "origin",
                skip_hooks: false,
                lfs_config: &[],
                lfs_objects: &[],
                receive_pack: "",
            },
            "C:/recorded/hooks".into(),
            Some(&creds),
        );
        assert!(result.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn recording_reviewed_push_policy_preserves_counted_alias_and_exact_subcommand()
    -> Result<(), String> {
        let address = "https://fixture.invalid/team/repo.git";
        let alias = "gitcraft-preflight-fixture";
        let creds = Credentials::for_url(address, "alice".into(), "DUMMY_ONLY".into())?;
        let source = "a".repeat(40);
        let refspec = format!("{source}:refs/heads/main");
        let words = [
            "push",
            "--porcelain",
            "--force-with-lease=refs/heads/main:",
            alias,
            &refspec,
        ];
        let target = |address| PushTarget {
            address,
            skip_hooks: false,
            alias,
            original_remote: "origin",
            lfs_config: &[],
            lfs_objects: &[],
            receive_pack: "",
        };
        let repo = Path::new("/nonexistent/recorded-reviewed-push");
        let extra = reviewed_push_extra(
            repo,
            &words,
            target(address),
            "/owned/pinned-hooks".into(),
            Some(&creds),
        )?;
        assert!(extra.normalize_repository);
        assert_eq!(
            extra.owned_pre.last().map(String::as_str),
            Some("gitcraft-credential-transport")
        );
        let mut command = Command::new("/recorded/git");
        command
            .arg("-C")
            .arg(repo)
            .env("GIT_DIR", "/foreign/repo")
            .env("GIT_CONFIG_COUNT", "99");
        let command = apply_command_extras(command, Some(repo), &extra, &words);
        let args = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.ends_with(&words.map(str::to_owned)));
        let push = args.len() - words.len();
        assert_eq!(args[push - 1], "gitcraft-credential-transport");
        assert!(!args.join(" ").contains("DUMMY_ONLY"));
        let env = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(env.get("GIT_DIR"), Some(&None));
        assert_eq!(env.get("GIT_CONFIG_COUNT"), Some(&Some("10".into())));
        assert_eq!(
            env.get("GITCRAFT_GIT_ORIGINAL"),
            Some(&Some("/recorded/git".into()))
        );
        assert!(!env.contains_key("GITCRAFT_CREDENTIAL_OWNER"));
        assert_eq!(
            env.get("GITCRAFT_GIT_PASSWORD"),
            Some(&Some("DUMMY_ONLY".into()))
        );
        let configs = (0..10)
            .map(|i| {
                let value = |prefix| {
                    let key = format!("{prefix}_{i}");
                    env.get(&key)
                        .and_then(Option::as_ref)
                        .cloned()
                        .ok_or_else(|| format!("missing {key}"))
                };
                Ok((value("GIT_CONFIG_KEY")?, value("GIT_CONFIG_VALUE")?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let placeholder = format!("gitcraft-preflight-address://{alias}");
        for remote in [alias, "origin"] {
            for field in ["url", "pushurl"] {
                assert_eq!(
                    configs
                        .iter()
                        .filter(|(k, _)| k == &format!("remote.{remote}.{field}"))
                        .map(|(_, v)| v.as_str())
                        .collect::<Vec<_>>(),
                    vec!["", placeholder.as_str()]
                );
            }
        }
        assert!(configs.contains(&(format!("url.{address}.insteadOf"), placeholder)));
        assert!(configs.contains(&("core.hooksPath".into(), "/owned/pinned-hooks".into())));
        for trace_hooks in [false, true] {
            let mut remote_extra = extra.clone();
            remote_extra.trace_hooks = trace_hooks;
            let remote = remote::Target {
                host: remote::Host::Ssh("uncontacted.invalid".into()),
                path: "/repo".into(),
            };
            let request = remote_request(&remote, &remote_extra, &words);
            assert!(request.script.starts_with(CREDENTIAL_REMOTE_PIN));
            assert!(request.script.contains("\"$GITCRAFT_GIT_ORIGINAL\""));
            assert!(
                request
                    .script
                    .contains("'gitcraft-credential-transport' 'push' '--porcelain'")
            );
            assert!(!request.script.contains("DUMMY_ONLY"));
            assert!(
                request
                    .env
                    .contains(&("GIT_CONFIG_COUNT".into(), "10".into()))
            );
            assert!(
                !request
                    .env
                    .iter()
                    .any(|(k, _)| k == "GITCRAFT_CREDENTIAL_OWNER")
            );
            assert!(request.unset.iter().any(|s| s == "GIT_CONFIG_COUNT"));
        }
        let other = reviewed_push_extra(
            repo,
            &words,
            target("https://other.invalid/repo"),
            "/owned/hooks".into(),
            Some(&creds),
        )?;
        assert!(other.owned_pre.is_empty());
        assert!(!other.env.iter().any(|(k, _)| k.starts_with("GITCRAFT_")));
        let edited = Credentials {
            username: "bob".into(),
            ..creds.clone()
        };
        assert!(
            reviewed_push_extra(
                repo,
                &words,
                target(address),
                "/hooks".into(),
                Some(&edited)
            )
            .is_err()
        );
        let legacy = Credentials {
            scope: None,
            ..creds
        };
        assert!(
            reviewed_push_extra(
                repo,
                &words,
                target("https://other.invalid/repo"),
                "/hooks".into(),
                Some(&legacy)
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn explicit_repository_context_clears_redirects_and_retains_private_index_overrides() {
        let private = Extra {
            normalize_repository: true,
            unset: vec!["GIT_INDEX_FILE", "GIT_CONFIG_COUNT", "GIT_SHALLOW_FILE"],
            env: vec![
                ("GIT_INDEX_FILE", "private.index".into()),
                ("GIT_CONFIG_COUNT", "1".into()),
                ("GIT_CONFIG_KEY_0", "remote.private.url".into()),
                ("GIT_CONFIG_VALUE_0", "https://push.invalid/repo".into()),
            ],
            ..Default::default()
        };
        let mut command = Command::new("/recorded/git");
        command.env("GIT_DIR", "/foreign/repo");
        clear_repository_environment(&mut command);
        let command =
            apply_command_extras(command, Some(Path::new("chosen")), &private, &["status"]);
        let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        for key in REPOSITORY_ENV_REMOVED {
            if !["GIT_INDEX_FILE", "GIT_CONFIG_COUNT"].contains(key) {
                assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None), "{key}");
            }
        }
        assert_eq!(
            env.get(std::ffi::OsStr::new("GIT_INDEX_FILE")),
            Some(&Some(std::ffi::OsStr::new("private.index")))
        );
        for (key, value) in &private.env {
            assert_eq!(
                env.get(std::ffi::OsStr::new(key)),
                Some(&Some(std::ffi::OsStr::new(value)))
            );
        }
        let target = remote::Target {
            host: remote::Host::Ssh("fixture.invalid".into()),
            path: "/chosen".into(),
        };
        let request = remote_request(&target, &private, &["status"]);
        for key in REPOSITORY_ENV_REMOVED {
            assert!(request.unset.iter().any(|s| s == key));
        }
        assert!(
            request
                .env
                .contains(&("GIT_INDEX_FILE".into(), "private.index".into()))
        );
    }

    #[test]
    fn safety_and_operation_reads_select_the_same_explicit_repository() -> Result<(), String> {
        let a = crate::push_preflight::tests::Fixture::new()?;
        let b = crate::push_preflight::tests::Fixture::new()?;
        let expected = a.git(&["rev-parse", "--absolute-git-dir"])?;
        // A separate owned command models the shell's inherited redirect without
        // changing the process-global environment used by concurrent tests.
        let inherited =
            std::process::Command::new(crate::shell_path::executable("git").ok_or("git missing")?)
                .env("GIT_DIR", b.repo.join(".git"))
                .arg("-C")
                .arg(&a.repo)
                .args(["rev-parse", "--absolute-git-dir"])
                .output()
                .map_err(|e| e.to_string())?;
        assert_ne!(String::from_utf8_lossy(&inherited.stdout).trim(), expected);
        let ordinary = local_command(
            Some(&a.repo),
            &Extra::default(),
            &["rev-parse", "--absolute-git-dir"],
        )
        .output()
        .map_err(|e| e.to_string())?;
        let safety = forecast_run(
            &a.repo,
            &["rev-parse", "--absolute-git-dir"],
            &[],
            &[0],
            &|| true,
        )?;
        assert!(ordinary.status.success());
        assert_eq!(String::from_utf8_lossy(&ordinary.stdout).trim(), expected);
        assert_eq!(safety.trim(), expected);
        Ok(())
    }
    #[test]
    fn recording_refusal_only_relays_known_status_for_the_reviewed_ref() -> Result<(), String> {
        let source = "a".repeat(40);
        let destination = "refs/heads/main";
        let good = format!(
            "server secret should disappear\n!\t{source}:{destination}\t[rejected] (stale info)\n"
        );
        let safe = reviewed_refusal(good.as_bytes(), &source, destination)
            .ok_or("missing good refusal")?;
        assert!(!safe.contains("secret"));
        assert!(safe.ends_with("[rejected] (stale info)"));
        let hook =
            format!("!\t{source}:{destination}\t[remote rejected] (password=hidden-secret)\n");
        let safe = reviewed_refusal(hook.as_bytes(), &source, destination)
            .ok_or("missing hook refusal")?;
        assert!(!safe.contains("hidden-secret"));
        assert!(safe.contains("[remote rejected]"));
        assert!(reviewed_refusal(good.as_bytes(), &source, "refs/heads/other").is_none());
        Ok(())
    }
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
pub(super) fn repository_command(dir: Option<&Path>, args: &[&str]) -> Command {
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
fn practice_command(dir: &Path) -> Command {
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

#[cfg(test)]
mod practice_environment_tests {
    use super::*;
    #[test]
    fn synthetic_practice_command_removes_every_inherited_route() {
        let cmd = practice_command(Path::new("sandbox"));
        let env: std::collections::HashMap<_, _> = cmd.get_envs().collect();
        for name in crate::practice::ISOLATED_ENV_REMOVE {
            assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None), "{name}");
        }
        for (name, value) in crate::practice::ISOLATED_ENV_SET {
            assert_eq!(
                env.get(std::ffi::OsStr::new(name)).copied().flatten(),
                Some(std::ffi::OsStr::new(value)),
                "{name}"
            );
        }
        assert!(cmd.get_args().any(|a| a == "--no-replace-objects"));
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

fn download_read_error(args: &[&str], error: String) -> String {
    if super::clone_options::passive_object_read(args)
        && [
            "missing blob",
            "missing object",
            "bad object",
            "unable to read",
            "could not read",
            "promisor",
        ]
        .iter()
        .any(|text| error.to_lowercase().contains(text))
    {
        crate::i18n::message(
            "backend-error-automatic-object-downloads-are-disabled-in-a-partial-clone-5cfc0a",
            &[("detail", (error).to_string().into())],
        )
    } else {
        error
    }
}

/// Shared by streaming, watched, bounded and ordinary local commands.
fn object_read_policy(cmd: &mut Command, args: &[&str]) {
    cmd.env(
        "GIT_NO_LAZY_FETCH",
        super::clone_options::download_environment(args),
    );
    if super::clone_options::passive_object_read(args) {
        cmd.arg("--no-lazy-fetch").env("GIT_NO_LAZY_FETCH", "1");
    }
}
#[cfg(test)]
mod download_policy_recording_tests {
    use super::*;
    #[test]
    fn passive_reads_are_guarded_locally_and_on_the_repository_machine() {
        for sub in [
            "show",
            "diff",
            "log",
            "cat-file",
            "blame",
            "status",
            "ls-tree",
            "sparse-checkout",
        ] {
            let args = [sub, "HEAD"];
            let mut cmd = Command::new("/recorded/git");
            object_read_policy(&mut cmd, &args);
            let cmd = apply_command_extras(
                cmd,
                Some(Path::new("/recorded/repo")),
                &Extra::default(),
                &args,
            );
            assert_eq!(
                cmd.get_args().next(),
                Some(std::ffi::OsStr::new("--no-lazy-fetch"))
            );
            let req = remote_request(
                &Target {
                    host: remote::Host::Ssh("recorded".into()),
                    path: "/repo".into(),
                },
                &Extra::default(),
                &args,
            );
            assert!(req.script.starts_with("exec git '--no-lazy-fetch'"));
        }
    }
}

#[cfg(all(test, unix))]
mod sparse_account_recording_tests {
    use super::*;
    #[test]
    fn consented_sparse_uses_captured_account_and_restores_passive_policy() -> Result<(), String> {
        let profile = crate::connections::Context {
            git_config: Some("/recorded/profile-a".into()),
            ..Default::default()
        };
        profile.run(|| {
            let account = Credentials::for_url(
                "https://example.invalid/team/repo",
                "account-A".into(),
                "SYNTHETIC_A_ONLY".into(),
            )?;
            // Same value captured by the Apply worker, even if the tab later signs in as B.
            let captured = account.clone();
            for repo in [
                Path::new("/recorded/repo"),
                Path::new("ssh://recorded/repo"),
            ] {
                for consent in [true, false] {
                    super::super::clone_options::sparse_mutation(
                        repo,
                        Some(&["src".into()]),
                        consent,
                        Some(&captured),
                        |dir, args, credentials| {
                            assert_eq!(dir, Some(repo));
                            assert_eq!(
                                credentials.map(|c| c.username.as_str()),
                                consent.then_some("account-A")
                            );
                            let extra = extra_for(dir, args, credentials, false)?;
                            let mut cmd = Command::new("/recorded/git");
                            cmd.env("GITCRAFT_GIT_USERNAME", "ambient-B");
                            object_read_policy(&mut cmd, args);
                            with_profile(&mut cmd, args);
                            // None avoids the filesystem-only practice-repository classifier;
                            // command discovery/launch is never entered by this recorder.
                            let cmd = apply_command_extras(cmd, None, &extra, args);
                            let env: std::collections::BTreeMap<_, _> = cmd
                                .get_envs()
                                .map(|(k, v)| {
                                    (
                                        k.to_string_lossy().into_owned(),
                                        v.map(|v| v.to_string_lossy().into_owned()),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                env.get("GIT_CONFIG_GLOBAL"),
                                Some(&Some("/recorded/profile-a".into()))
                            );
                            assert_eq!(
                                env.get("GIT_NO_LAZY_FETCH"),
                                Some(&Some(if consent { "0" } else { "1" }.into()))
                            );
                            if consent {
                                assert_eq!(
                                    env.get("GITCRAFT_GIT_USERNAME"),
                                    Some(&Some("account-A".into()))
                                );
                                assert_eq!(
                                    env.get("GITCRAFT_GIT_PASSWORD"),
                                    Some(&Some("SYNTHETIC_A_ONLY".into()))
                                );
                            } else {
                                assert!(
                                    !extra
                                        .env
                                        .iter()
                                        .any(|(key, _)| *key == "GITCRAFT_GIT_PASSWORD")
                                );
                            }
                            let argv: Vec<_> = cmd
                                .get_args()
                                .map(|a| a.to_string_lossy().into_owned())
                                .collect();
                            assert_eq!(
                                argv.iter().any(|a| a == "gitcraft-credential-transport"),
                                consent
                            );
                            assert_eq!(argv.iter().any(|a| a == "--no-lazy-fetch"), !consent);
                            assert!(!argv.join(" ").contains("SYNTHETIC_A_ONLY"));
                            let req = remote_request(
                                &Target {
                                    host: remote::Host::Ssh("recorded".into()),
                                    path: "/repo".into(),
                                },
                                &extra,
                                args,
                            );
                            assert_eq!(
                                req.env
                                    .iter()
                                    .any(|(k, v)| k == "GITCRAFT_GIT_USERNAME" && v == "account-A"),
                                consent
                            );
                            assert_eq!(
                                req.script.contains("gitcraft-credential-transport"),
                                consent
                            );
                            assert_eq!(req.script.contains("--no-lazy-fetch"), !consent);
                            assert!(!req.script.contains("SYNTHETIC_A_ONLY"));
                            Ok(String::new())
                        },
                    )?;
                }
            }
            for args in [["rev-parse", "HEAD"], ["show", "HEAD:file"]] {
                let extra = extra_for(None, &args, None, false)?;
                assert!(extra.env.is_empty());
                let mut cmd = Command::new("/recorded/git");
                object_read_policy(&mut cmd, &args);
                let cmd = apply_command_extras(cmd, None, &extra, &args);
                assert_eq!(
                    cmd.get_args().next(),
                    Some(std::ffi::OsStr::new("--no-lazy-fetch"))
                );
                assert!(!cmd.get_envs().any(|(k, _)| k == "GITCRAFT_GIT_PASSWORD"));
            }
            Ok(())
        })
    }
}

/// Pin an already reviewed effective endpoint using a private remote. Git expands
/// the private placeholder once; explicit pushurl disables pushInsteadOf. The
/// effective address is never handed back as an unqualified remote/URL operand.
fn repository_endpoint_extra(
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

#[cfg(test)]
mod repository_endpoint_recording {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn reviewed_endpoint_and_account_survive_normalization_without_reinterpretation()
    -> Result<(), String> {
        let repo = Path::new("C:/recorded/repo");
        let url = "https://host-a.invalid/team/repo";
        let creds = Credentials::for_url(url, "alice".into(), "DUMMY_A_TOKEN".into())?;
        let result = repository_endpoint_extra(repo, url, "origin", Some(&creds), &["push"]);
        assert!(result.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
        // An account scoped to another host needs no transient credential transport.
        let (extra, _) = repository_endpoint_extra(
            repo,
            "https://host-b.invalid/team/repo",
            "origin",
            Some(&creds),
            &["push"],
        )?;
        assert!(extra.owned_pre.is_empty());
        assert!(!extra.env.iter().any(|(k, _)| k.starts_with("GITCRAFT_")));
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn reviewed_endpoint_and_account_survive_normalization_without_reinterpretation()
    -> Result<(), String> {
        let repo = Path::new("/recorded/repo");
        let url = "https://host-a.invalid/team/repo";
        let credentials = Credentials::for_url(url, "alice".into(), "DUMMY_A_TOKEN".into())?;
        for address in [url, "https://host-b.invalid/team/repo", "origin"] {
            let (extra, alias) =
                repository_endpoint_extra(repo, address, "origin", Some(&credentials), &["push"])?;
            let args = ["push", "--porcelain", "--", &alias, "a:refs/heads/main"];
            let mut selected = Command::new("/recorded/chosen/git");
            clear_repository_environment(&mut selected);
            let cmd = apply_command_extras(selected, None, &extra, &args);
            let env: std::collections::BTreeMap<_, _> = cmd.get_envs().collect();
            let get = |k: &str| env.get(std::ffi::OsStr::new(k)).copied().flatten();
            assert_eq!(get("GIT_CONFIG_COUNT"), Some(std::ffi::OsStr::new("9")));
            assert_eq!(
                get("GIT_CONFIG_KEY_0"),
                Some(std::ffi::OsStr::new(&format!("url.{address}.insteadOf")))
            );
            assert_eq!(get("GIT_CONFIG_VALUE_0"), get("GIT_CONFIG_VALUE_2"));
            assert_eq!(get("GIT_CONFIG_VALUE_2"), get("GIT_CONFIG_VALUE_4"));
            assert_eq!(get("GIT_CONFIG_VALUE_4"), get("GIT_CONFIG_VALUE_8"));
            for name in REPOSITORY_ENV_REMOVED {
                if *name != "GIT_CONFIG_COUNT" && *name != "GIT_CONFIG_GLOBAL" {
                    assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None), "{name}");
                }
            }
            assert_eq!(get("GITCRAFT_GIT_PASSWORD").is_some(), address == url);
            if address == url {
                assert_eq!(
                    get("GITCRAFT_GIT_ORIGINAL"),
                    Some(std::ffi::OsStr::new("/recorded/chosen/git"))
                );
            }
            assert!(
                !cmd.get_args()
                    .any(|a| a.to_string_lossy().contains("DUMMY_A_TOKEN"))
            );
            let target = remote::Target {
                host: remote::Host::Ssh("uncontacted.invalid".into()),
                path: "/repo".into(),
            };
            let request = remote_request(&target, &extra, &args);
            assert_eq!(
                request.script.starts_with(CREDENTIAL_REMOTE_PIN),
                address == url
            );
            assert!(request.unset.iter().any(|k| k == "GIT_CONFIG_COUNT"));
            assert!(
                request
                    .env
                    .contains(&("GIT_CONFIG_COUNT".into(), "9".into()))
            );
        }
        let legacy = Credentials {
            scope: None,
            ..credentials.clone()
        };
        assert!(repository_endpoint_extra(repo, url, "origin", Some(&legacy), &["push"]).is_err());
        let edited = Credentials {
            username: "bob".into(),
            ..credentials
        };
        assert!(repository_endpoint_extra(repo, url, "origin", Some(&edited), &["push"]).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod internals_merge_recording {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn root_unset_and_profile_precede_both_pinned_transport_environments() -> Result<(), String> {
        let url = "https://fixture.invalid/team/repo";
        let repo = Path::new("C:/recorded/repo");
        let creds = Credentials::for_url(url, "alice".into(), "DUMMY_MERGE".into())?;
        let context = crate::connections::Context {
            git_config: Some("C:/recorded/initiating-profile".into()),
            ..Default::default()
        };
        context.run(|| {
            let internals = repository_endpoint_extra(repo, url, "origin", Some(&creds), &["push"]);
            assert!(
                internals.is_err_and(|e| e.contains("native/MSYS transport identity boundary"))
            );
            let reviewed = reviewed_push_extra(
                repo,
                &["push"],
                PushTarget {
                    address: url,
                    alias: "reviewed-fixture",
                    original_remote: "origin",
                    skip_hooks: false,
                    lfs_config: &[],
                    lfs_objects: &[],
                    receive_pack: "",
                },
                "C:/recorded/hooks".into(),
                Some(&creds),
            );
            assert!(reviewed.is_err_and(|e| e.contains("native/MSYS transport identity boundary")));
        });
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn root_unset_and_profile_precede_both_pinned_transport_environments() -> Result<(), String> {
        let url = "https://fixture.invalid/team/repo";
        let repo = Path::new("/recorded/repo");
        let creds = Credentials::for_url(url, "alice".into(), "DUMMY_MERGE".into())?;
        let context = crate::connections::Context {
            git_config: Some("/recorded/initiating-profile".into()),
            ..Default::default()
        };
        context.run(|| -> Result<(), String> {
            let (internals, alias) =
                repository_endpoint_extra(repo, url, "origin", Some(&creds), &["push"])?;
            let reviewed = reviewed_push_extra(
                repo,
                &["push"],
                PushTarget {
                    address: url,
                    alias: "reviewed-fixture",
                    original_remote: "origin",
                    skip_hooks: false,
                    lfs_config: &[],
                    lfs_objects: &[],
                    receive_pack: "",
                },
                "/recorded/hooks".into(),
                Some(&creds),
            )?;
            for (mut extra, count, endpoint_alias) in [
                (internals, "9", alias.as_str()),
                (reviewed, "10", "reviewed-fixture"),
            ] {
                extra.unset.extend(["GIT_INDEX_FILE", "GIT_CONFIG_COUNT"]);
                extra.env.extend([
                    ("GIT_INDEX_FILE", "/recorded/private-index".into()),
                    ("GIT_DIR", "/recorded/command-source".into()),
                ]);
                let mut command = Command::new("/recorded/selected/git");
                command
                    .arg("-C")
                    .arg(repo)
                    .env("GIT_DIR", "/foreign")
                    .env("GIT_CONFIG_COUNT", "99");
                clear_repository_environment(&mut command);
                let words = [
                    "push",
                    "--porcelain",
                    "--",
                    endpoint_alias,
                    "a:refs/heads/dev",
                ];
                // All program/directory strings are inert. None avoids discovery
                // and the native practice-registry probe during this recording.
                let command = apply_command_extras(command, None, &extra, &words);
                let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
                let value = |key: &str| env.get(std::ffi::OsStr::new(key)).copied().flatten();
                assert_eq!(value("GIT_CONFIG_COUNT"), Some(std::ffi::OsStr::new(count)));
                assert_eq!(
                    value("GIT_INDEX_FILE"),
                    Some(std::ffi::OsStr::new("/recorded/private-index"))
                );
                assert_eq!(
                    value("GIT_DIR"),
                    Some(std::ffi::OsStr::new("/recorded/command-source"))
                );
                assert_eq!(
                    value("GIT_CONFIG_GLOBAL"),
                    Some(std::ffi::OsStr::new("/recorded/initiating-profile"))
                );
                assert_eq!(
                    value("GITCRAFT_GIT_ORIGINAL"),
                    Some(std::ffi::OsStr::new("/recorded/selected/git"))
                );
                assert_eq!(
                    value("GITCRAFT_GIT_PASSWORD"),
                    Some(std::ffi::OsStr::new("DUMMY_MERGE"))
                );
                assert_eq!(value("GIT_GRAFT_FILE"), None);
                let argv: Vec<_> = command.get_args().collect();
                assert!(argv.iter().any(|a| *a == "gitcraft-credential-transport"));
                assert!(
                    !argv
                        .iter()
                        .any(|a| a.to_string_lossy().contains("DUMMY_MERGE"))
                );
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod backend_fluent_identity_tests {
    #[test]
    fn backend_fluent_missing_folder_keeps_recent_status_identity() {
        let target = crate::remote::Target {
            host: crate::remote::Host::Ssh("fixture-host".into()),
            path: "/fixture/de/repo".into(),
        };
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            let raw = super::missing_folder(&target);
            assert_eq!(raw, "/fixture/de/repo isn't a folder on SSH · fixture-host");
            assert!(raw.to_lowercase().contains("isn't a folder on"));
            assert_eq!(crate::across::short_reason(&raw), "folder not found");
            assert_eq!(
                crate::git::display_error(&raw),
                if locale == "de" {
                    "/fixture/de/repo ist kein Ordner auf SSH · fixture-host."
                } else {
                    &raw
                }
            );
        }
        crate::i18n::set_language("en-US");
    }
}
