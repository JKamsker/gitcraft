//! Running git: the environment every call gets, credentials for remotes, errors. A call for a
//! repository in WSL or over SSH runs git there (see [`crate::remote`]), with the same
//! environment.

pub(super) mod bounded;

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
fn after_options<'a>(args: &'a [&'a str]) -> &'a [&'a str] {
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
pub(crate) use credential_policy::*;
pub(crate) use endpoints::*;
mod setup;
pub(crate) use connection::*;
pub(crate) use setup::{setup_clone, setup_fetch, setup_run};
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
    /// Opt-in captured connection reads/recovery use the selected repository/profile,
    /// not repository/config redirection inherited from the launching terminal.
    normalize_repository: bool,
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
    /// On another machine: a test of a parallel bisect (see [`run_for_test`]).
    test: Option<std::time::Duration>,
    /// How long it may run on another machine before it is stopped there.
    timeout: Option<std::time::Duration>,
}

/// How long a diff or merge tool started on another machine may stay open before it is stopped.
const TOOL_FOR: std::time::Duration = std::time::Duration::from_secs(12 * 3600);

/// Runs git with `args` in `dir` (on whichever machine it is), `extra` before them.
fn execute(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    let _timer = super::backend::CliTimer::new(args);
    if let Some(target) = dir.and_then(remote::parse) {
        let result = execute_there(&target, extra, args);
        if let Ok(out) = &result {
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
                .map_err(|e| format!("could not save output: {e}"))?,
            output_limit: extra.output_limit.unwrap_or(64 << 20),
            background: extra.long || extra.tool || extra.test.is_some() || goes_over_network(args),
            timeout: extra.timeout,
            ..Default::default()
        },
    )?
    .complete()?;
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
    cmd.args(&extra.pre).args(&extra.owned_pre);
    if extra.normalize_repository && !dir.is_some_and(crate::practice::is_repository) {
        normalize_connection_command(&mut cmd, args);
    }
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
    let words: Vec<String> = extra
        .pre
        .iter()
        .copied()
        .chain(extra.owned_pre.iter().map(String::as_str))
        .chain(args.iter().copied())
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
            .chain(extra.env.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .collect(),
        unset: GIT_ENV_REMOVED
            .iter()
            .copied()
            .chain(
                extra
                    .normalize_repository
                    .then_some(CONNECTION_REDIRECT_ENV)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .map(str::to_owned)
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

fn execute_there(target: &Target, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    let out = remote::exec(&target.host, &remote_request(target, extra, args))?;
    // The folder is gone (or never was) on that machine.
    if out.code == 1 && remote::is_missing_folder(&out.stderr) {
        return Err(format!(
            "{} isn't a folder on {}",
            target.path,
            target.host.label()
        ));
    }
    let (stderr, events) = if extra.trace_hooks {
        super::hooks::split_trace(&out.stderr)
    } else {
        (out.stderr, String::new())
    };
    Ok(Ran {
        code: Some(out.code),
        status: format!("exit status: {}", out.code),
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
        Err(format!(
            "{} isn't a folder on {}",
            target.path,
            target.host.label()
        ))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
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
                return Err(format!(
                    "{} isn't a folder on {}",
                    target.path,
                    target.host.label()
                ));
            }
            Ran {
                code: Some(out.code),
                status: format!("exit status: {}", out.code),
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
            format!("git {} failed ({})", args.join(" "), out.status)
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
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Save a command's stdout without capturing a local export in memory. Remote exports
/// retain the existing persistent-shell transport and its binary-output behavior.
pub fn run_to_file(dir: &Path, args: &[&str], file: &Path) -> Result<(), String> {
    if remote::is_remote(dir) {
        return run_bytes(Some(dir), args).and_then(|bytes| {
            std::fs::write(file, bytes).map_err(|e| format!("could not save it: {e}"))
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
                format!("git {} failed ({})", args.join(" "), out.status)
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

/// Starts git in `dir` with its output piped, for a caller that reads it bit by bit.
pub(super) fn spawn_piped(dir: &Path, args: &[&str]) -> Result<std::process::Child, String> {
    if let Some(target) = remote::parse(dir) {
        return remote::spawn(
            &target.host,
            &remote_request(&target, &Extra::default(), args),
        );
    }
    let mut cmd = repository_command(Some(dir), args);
    cmd.args(args);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.spawn().map_err(|e| format!("could not run git: {e}"))
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
        format!("git {} failed ({})", args.join(" "), out.status)
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

/// Runs git with `args` in `dir` for a parallel bisect's tests, whatever it exits with. On
/// another machine it gets a shell of its own (as many run side by side as there are tests at
/// once) and is stopped there after `limit`; here it runs as long as it takes, and the caller
/// stops it (see `parallel_bisect::runner`).
pub fn run_for_test(dir: &Path, args: &[&str], limit: std::time::Duration) -> Result<Exit, String> {
    let extra = Extra {
        test: Some(limit),
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    Ok(Exit {
        code: out.code,
        stdout: out.stdout,
        stderr: out.stderr,
    })
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

/// A designated HTTP transport helper; explicit transient accounts replace ambient
/// helpers and are never automatically stored. Secret values stay out of argv.
/// `printf '%s'`, not `echo`: the `sh` git runs it with (dash…) would read backslashes in them
/// as escapes (`CORP\nancy` broken in two lines, `\c` cutting the password short).
const ENV_HELPER: &str = super::CREDENTIAL_HELPER;

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
        (true, true) => format!("git {} failed ({status})", args.join(" ")),
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
        Err(match (&out.failed_hook, said.is_empty()) {
            (Some(hook), _) => hook.explain(&said, args),
            (None, true) => format!("git {} failed ({})", args.join(" "), out.status),
            (None, false) => said,
        })
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
        return Err(
            "Syntax comparison is currently available for local repositories. Showing the patch."
                .to_owned(),
        );
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
        format!("Syntax preparation requires Git with --no-lazy-fetch support: {error}")
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
        return Err("Comparison output exceeds the size limit; choose fewer files".into());
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
            req.script.starts_with("exec git 'mergetool'"),
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
        return practice_command(dir);
    }
    let mut cmd = git_cmd();
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
        return Err("Practice repositories are local".into());
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
        return Err("Unsupported setup configuration key".into());
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
        return Err("Could not inspect local setup configuration".into());
    }
    let value = bytes
        .stdout
        .strip_suffix(&[0])
        .ok_or("Invalid configuration response")?;
    String::from_utf8(value.to_vec())
        .map(Some)
        .map_err(|_| "Local configuration is not UTF-8".into())
}
