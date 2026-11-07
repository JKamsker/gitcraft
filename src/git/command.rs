//! Running git: the environment every call gets, credentials for remotes, errors. A call for a
//! repository in WSL or over SSH runs git there (see [`crate::remote`]), with the same
//! environment.

pub(super) mod bounded;

use std::path::Path;
use std::process::Command;

/// Whether `args` reads or writes the user's own global config: `git config --global …`
/// (signing settings, Mergiraf's set-up), which must never land in the profile's file, which is
/// written over.
fn user_global_config(args: &[&str]) -> bool {
    let rest = after_options(args);
    rest.first() == Some(&"config") && rest.contains(&"--global")
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
fn with_profile(cmd: &mut Command, args: &[&str]) -> Result<(), String> {
    if user_global_config(args) {
        return Ok(());
    }
    let context = crate::connections::current();
    if let Some(error) = context.error {
        return Err(error);
    }
    if let Some(path) = context.git_config {
        cmd.env("GIT_CONFIG_GLOBAL", path);
    }
    if !context.choices.ssh_key.is_empty() {
        cmd.env_remove("GIT_SSH_COMMAND")
            .env_remove("GIT_SSH")
            .env_remove("GIT_SSH_VARIANT");
    }
    Ok(())
}

use crate::remote::{self, Target};

/// What every git call is run with.
/// - `GIT_TERMINAL_PROMPT`: never block on an interactive prompt (credential helpers with a
///   GUI still work).
/// - `GIT_EDITOR`: `rebase --continue`, `merge --continue`… would otherwise open an editor
///   nobody sees.
/// - `LANGUAGE`: messages in English whatever the user's language: errors are told apart by
///   their words (`is_auth_error`…). `C` in LANGUAGE wins over LC_ALL / LANG for messages only,
///   so the character set (and what hooks see of it) stays the user's.
const GIT_ENV: [(&str, &str); 4] = [
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_EDITOR", "true"),
    ("LANGUAGE", "C"),
];

/// Interactive rebases set `sequence.editor` themselves; an inherited one would win.
const GIT_ENV_REMOVED: [&str; 1] = ["GIT_SEQUENCE_EDITOR"];

fn git_cmd() -> Command {
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
}

/// Options before the subcommand, and variables, a call is run with on top of [`GIT_ENV`].
#[derive(Default)]
struct Extra {
    pre: Vec<&'static str>,
    env: Vec<(&'static str, String)>,
}

/// Construct every local repository process through one route, including piped logs.
/// Practice detection reads bounded marker metadata, never Git or inherited configuration.
fn repository_command(dir: Option<&Path>, args: &[&str]) -> Result<Command, String> {
    if let Some(dir) = dir.filter(|dir| crate::practice::is_repository(dir)) {
        return Ok(practice_command(dir));
    }
    let mut cmd = git_cmd();
    with_profile(&mut cmd, args)?;
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    Ok(cmd)
}

/// Runs git with `args` in `dir` (on whichever machine it is), `extra` before them.
fn execute(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    if let Some(target) = dir.and_then(remote::parse) {
        return execute_there(&target, extra, args);
    }
    let mut cmd = repository_command(dir, args)?;
    cmd.args(&extra.pre);
    for (name, value) in &extra.env {
        cmd.env(name, value);
    }
    cmd.args(args);
    let out = cmd
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    Ok(Ran {
        code: out.status.code(),
        status: out.status.to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// The request that runs git with `args` in `target`'s folder, on its machine.
fn remote_request(target: &Target, extra: &Extra, args: &[&str]) -> remote::Request {
    let words: Vec<String> = extra
        .pre
        .iter()
        .copied()
        .chain(args.iter().copied())
        .map(remote::sh_quote)
        .collect();
    remote::Request {
        cwd: Some(target.path.clone()),
        script: format!("exec git {}", words.join(" ")),
        env: GIT_ENV
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .chain(extra.env.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .collect(),
        unset: GIT_ENV_REMOVED.iter().map(|n| (*n).to_owned()).collect(),
        stdin: None,
        long: goes_over_network(args),
        timeout: None,
    }
}

/// Whether the git command `args` (its options first) talks to a remote and may take minutes.
fn goes_over_network(args: &[&str]) -> bool {
    matches!(
        after_options(args).first().copied(),
        Some("fetch" | "pull" | "push" | "clone" | "ls-remote" | "remote" | "submodule" | "lfs")
    )
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
    Ok(Ran {
        code: Some(out.code),
        status: format!("exit status: {}", out.code),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Runs git in `dir` and returns stdout, or the error output.
pub fn run(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    run_ok_codes(dir, args, &[0])
}

/// Setup/read-only checks of a local practice sandbox. Never called for user repositories.
/// Inherited git directory overrides and global hooks/URL rewrites must not escape it.
pub(crate) fn practice_run(dir: &Path, args: &[&str]) -> Result<String, String> {
    if remote::is_remote(dir) {
        return Err("Practice repositories are local".to_owned());
    }
    let mut cmd = practice_command(dir);
    cmd.args(args);
    let output = cmd
        .output()
        .map_err(|e| format!("Could not run practice git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
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

/// Runs git and returns stdout as raw bytes (file contents: images…).
pub fn run_bytes(dir: Option<&Path>, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = execute(dir, &Extra::default(), args)?;
    if out.code == Some(0) {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
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
    let mut cmd = repository_command(Some(dir), args)?;
    cmd.args(args);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.spawn().map_err(|e| format!("could not run git: {e}"))
}

/// Like [`run`], but exit codes in `ok` count as success (`diff --no-index` exits 1 on changes).
pub fn run_ok_codes(dir: Option<&Path>, args: &[&str], ok: &[i32]) -> Result<String, String> {
    run_inner(dir, args, ok, None, false)
}

/// Only automatic rebase forecasts use this bounded, cancellable command path. Remote work
/// is detached immediately on cancellation and stopped by the remote session's own deadline.
pub(crate) fn forecast_bytes(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    use std::time::Duration;
    const LIMIT: Duration = Duration::from_secs(15);
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
    if !active() {
        return Err("Forecast cancelled".into());
    }
    let out = if let Some(target) = remote::parse(dir) {
        let extra = Extra {
            pre: Vec::new(),
            env: env.to_vec(),
        };
        let mut request = remote_request(&target, &extra, args);
        request.unset.extend(REMOVE.iter().map(|s| (*s).to_owned()));
        forecast_remote(&target.host, request, active)?
    } else {
        let mut command = git_cmd();
        with_profile(&mut command, args)?;
        command.arg("-C").arg(dir).args(args);
        for name in REMOVE {
            command.env_remove(name);
        }
        for (name, value) in env {
            command.env(name, value);
        }
        let out = crate::bounded_process::run(command, active, LIMIT)?;
        remote::Output {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        }
    };
    if ok.contains(&out.code) {
        Ok(out.stdout)
    } else {
        Err(format!(
            "Forecast git command failed ({}): {}",
            out.code,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub(crate) fn forecast_remote(
    host: &remote::Host,
    mut request: remote::Request,
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    const LIMIT: Duration = Duration::from_secs(15);
    if !active() {
        return Err("Forecast cancelled".into());
    }
    request.timeout = Some(LIMIT);
    request.long = true;
    let host = host.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(remote::exec(&host, &request));
    });
    let deadline = Instant::now() + LIMIT;
    loop {
        if !active() {
            return Err("Forecast cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("Forecast command timed out".into());
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(out) => return out,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("Forecast command stopped".into()),
        }
    }
}

pub(crate) fn forecast_run(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    forecast_bytes(dir, args, env, ok, active).map(|out| String::from_utf8_lossy(&out).into_owned())
}

/// A username and password (or token) for an HTTPS remote, typed into gitcraft.
#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// A credential helper that answers git's `get` from the environment, so the secret never shows
/// up in a command line. git tries the user's own helpers first and, when the operation works,
/// asks them to store what this one gave.
/// `printf '%s'`, not `echo`: the `sh` git runs it with (dash…) would read backslashes in them
/// as escapes (`CORP\nancy` broken in two lines, `\c` cutting the password short).
const ENV_HELPER: &str = "credential.helper=!f() { test \"$1\" = get && printf 'username=%s\\npassword=%s\\n' \"$GITCRAFT_GIT_USERNAME\" \"$GITCRAFT_GIT_PASSWORD\"; }; f";

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
    }
}

/// Sets `cmd` to open no sign-in window (see [`unattended_extra`]).
fn unattended(cmd: &mut Command) {
    let extra = unattended_extra(cfg!(unix));
    cmd.args(&extra.pre);
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

fn run_inner(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
) -> Result<String, String> {
    let mut extra = Extra::default();
    if quiet {
        // Unix on the machine git runs on: another one is always Linux (WSL, an SSH server).
        let unix = cfg!(unix) || dir.is_some_and(remote::is_remote);
        extra = unattended_extra(unix);
    }
    if let Some(c) = creds {
        extra.pre.extend(["-c", ENV_HELPER]);
        extra
            .env
            .push(("GITCRAFT_GIT_USERNAME", c.username.clone()));
        extra
            .env
            .push(("GITCRAFT_GIT_PASSWORD", c.password.clone()));
    }
    let out = execute(dir, &extra, args)?;
    if out.code.is_some_and(|c| ok.contains(&c)) {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        // Merges and pulls report conflicts on stdout and the summary on stderr.
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let err = match (stderr.is_empty(), stdout.is_empty()) {
            (true, true) => format!("git {} failed ({})", args.join(" "), out.status),
            (false, true) => stderr,
            (true, false) => stdout,
            (false, false) => format!("{stderr}\n{stdout}"),
        };
        Err(err)
    }
}

/// What git's credential helpers keep for `url` (`https://host`), asking nobody: no terminal
/// prompt, no askpass program, no sign-in window.
pub fn stored_credentials(url: &str) -> Option<Credentials> {
    use std::io::Write;
    use std::process::Stdio;
    let mut cmd = git_cmd();
    with_profile(&mut cmd, &["credential", "fill"]).ok()?;
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
    parse_credentials(&String::from_utf8_lossy(&out.stdout))
}

/// The name and password in what `git credential fill` prints.
fn parse_credentials(out: &str) -> Option<Credentials> {
    let field = |name: &str| {
        out.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(str::to_owned)
    };
    let (username, password) = (field("username")?, field("password")?);
    (!password.is_empty()).then_some(Credentials { username, password })
}

/// Read-only, bounded background analysis transport. Cancellation never holds a newer view.
pub(super) fn activity_bytes_with_limit(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
    limit: std::time::Duration,
) -> Result<Vec<u8>, String> {
    activity_bytes_with_codes(dir, args, input, active, limit, &[0])
}

fn activity_bytes_with_codes(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
    limit: std::time::Duration,
    ok: &[i32],
) -> Result<Vec<u8>, String> {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    const MAX: usize = 16 * 1024 * 1024;
    if !active() {
        return Err("Analysis cancelled".into());
    }
    let out = if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, &Extra::default(), args);
        // The shared remote runner captures whole outputs. Bound both streams *there*,
        // before they cross the connection or allocate a large local buffer. FIFO readers
        // cap scratch files and give Git SIGPIPE on overflow; preserve Git's exit status.
        let git = request
            .script
            .strip_prefix("exec ")
            .unwrap_or(&request.script)
            .to_owned();
        request.script = format!(
            concat!(
                "umask 077\n",
                "history_tmp=$(mktemp -d \"${{TMPDIR:-/tmp}}/gitcraft-activity.XXXXXXXX\") || exit 1\n",
                "trap 'rm -rf -- \"$history_tmp\"' 0\n",
                "mkfifo \"$history_tmp/out\" \"$history_tmp/err\" || exit 1\n",
                "head -c {cap} <\"$history_tmp/out\" >\"$history_tmp/out-cap\" & history_out=$!\n",
                "head -c {cap} <\"$history_tmp/err\" >\"$history_tmp/err-cap\" & history_err=$!\n",
                "{git} >\"$history_tmp/out\" 2>\"$history_tmp/err\"; history_code=$?\n",
                "wait \"$history_out\"; wait \"$history_err\"\n",
                "cat \"$history_tmp/out-cap\"; cat \"$history_tmp/err-cap\" >&2\n",
                "exit \"$history_code\""
            ),
            cap = MAX + 1,
            git = git
        );
        request.stdin = input.map(<[u8]>::to_vec);
        request.timeout = Some(limit);
        request.long = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(remote::exec(&target.host, &request));
        });
        let deadline = Instant::now() + limit;
        loop {
            if !active() {
                return Err("Analysis cancelled".into());
            }
            if Instant::now() >= deadline {
                return Err("History analysis timed out".into());
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(out) => break out?,
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(_) => return Err("History worker stopped".into()),
            }
        }
    } else {
        let mut command = repository_command(Some(dir), args)?;
        command.args(args);
        let output = crate::bounded_process::run_with_input(
            command,
            input.map(<[u8]>::to_vec),
            active,
            limit,
        )
        .map_err(|e| {
            if e.contains("output exceeded") {
                "A history page exceeded 16 MiB".to_owned()
            } else {
                e.replace("Forecast", "History analysis")
                    .replace("forecast", "history analysis")
            }
        })?;
        remote::Output {
            code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        }
    };
    if out.stdout.len() > MAX || out.stderr.len() > MAX {
        return Err("A history page exceeded 16 MiB".into());
    }
    if ok.contains(&out.code) && out.stderr.is_empty() {
        Ok(out.stdout)
    } else {
        Err(format!(
            "Could not analyze history: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// An absent option (exit 1) differs from an explicitly empty configured value (NUL).
pub(super) fn activity_mailmap_config(
    dir: &Path,
    key: &str,
    path: bool,
    active: &dyn Fn() -> bool,
) -> Result<Option<String>, String> {
    let mut args = vec!["config", "--null"];
    if path {
        args.push("--path");
    }
    args.extend(["--get", key]);
    let bytes = activity_bytes_with_codes(
        dir,
        &args,
        None,
        active,
        std::time::Duration::from_secs(20),
        &[0, 1],
    )?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let value = bytes
        .strip_suffix(&[0])
        .ok_or("Invalid mailmap configuration response")?;
    String::from_utf8(value.to_vec())
        .map(Some)
        .map_err(|_| "Mailmap configuration is not UTF-8".into())
}

pub(crate) fn activity_bytes(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    activity_bytes_with_limit(dir, args, input, active, std::time::Duration::from_secs(20))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::process::Stdio;

    use super::*;

    /// Network operations go to shells of their own on another machine; reads don't.
    #[test]
    fn network_operations_are_told_apart() {
        for (args, long) in [
            (&["fetch", "--prune"][..], true),
            (&["-c", "credential.helper=x", "push", "origin"][..], true),
            (&["--literal-pathspecs", "pull"][..], true),
            (&["submodule", "update"][..], true),
            (&["status", "--porcelain"][..], false),
            (&["-c", "fetch.x=1", "log"][..], false),
            (&["--no-optional-locks", "status"][..], false),
            (&[][..], false),
        ] {
            assert_eq!(goes_over_network(args), long, "{args:?}");
        }
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
        assert!(!user_global_config(&[]));
    }

    /// What git gets from the helper is what was typed, backslashes (`DOMAIN\user`) included.
    #[test]
    fn credentials_reach_git_as_typed() -> Result<(), String> {
        let (user, pass) = ("CORP\\nancy", "p\\c%s$x 'q\"");
        let mut child = git_cmd()
            // Only this helper: the user's own would answer first.
            .args([
                "-c",
                "credential.helper=",
                "-c",
                ENV_HELPER,
                "credential",
                "fill",
            ])
            .env("GITCRAFT_GIT_USERNAME", user)
            .env("GITCRAFT_GIT_PASSWORD", pass)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(b"protocol=https\nhost=example.invalid\n\n")
                .map_err(|e| e.to_string())?;
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains(&format!("username={user}\n")), "{text}");
        assert!(text.contains(&format!("password={pass}\n")), "{text}");
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
    #[test]
    fn typed_credentials_answer_a_quiet_operation() -> Result<(), String> {
        let mut cmd = git_cmd();
        unattended(&mut cmd);
        let mut child = cmd
            .args([
                "-c",
                "credential.helper=",
                "-c",
                ENV_HELPER,
                "credential",
                "fill",
            ])
            .env("GITCRAFT_GIT_USERNAME", "nancy")
            .env("GITCRAFT_GIT_PASSWORD", "secret")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(b"protocol=https\nhost=example.invalid\n\n")
                .map_err(|e| e.to_string())?;
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains("username=nancy\n"), "{text}");
        assert!(text.contains("password=secret\n"), "{text}");
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

#[cfg(test)]
mod practice_tests {
    use super::*;

    #[test]
    fn practice_commands_cannot_inherit_a_caller_repository_or_global_configuration() {
        let command = practice_command(Path::new("sandbox"));
        let env: std::collections::HashMap<_, _> = command.get_envs().collect();
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_CONFIG_PARAMETERS",
            "GIT_TEMPLATE_DIR",
        ] {
            assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None), "{name}");
        }
        for (name, value) in [
            ("GIT_CONFIG_GLOBAL", ""),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_COUNT", "0"),
        ] {
            assert_eq!(
                env.get(std::ffi::OsStr::new(name)).copied().flatten(),
                Some(std::ffi::OsStr::new(value)),
                "{name}"
            );
        }
    }
}

#[cfg(test)]
mod profile_connection_tests {
    use super::*;
    use crate::connections::{Choices, Context};

    #[test]
    fn local_git_gets_the_pinned_profile_but_remote_git_does_not_get_local_paths_or_accounts() {
        let context = Context {
            git_config: Some("/tmp/profile-work.gitconfig".into()),
            choices: Choices {
                ssh_key: "/tmp/work key".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        context.run(|| {
            let mut cmd = git_cmd();
            assert!(with_profile(&mut cmd, &["push"]).is_ok());
            let env: std::collections::HashMap<_, _> = cmd.get_envs().collect();
            assert_eq!(
                env.get(std::ffi::OsStr::new("GIT_CONFIG_GLOBAL")),
                Some(&Some(std::ffi::OsStr::new("/tmp/profile-work.gitconfig")))
            );
            assert_eq!(
                env.get(std::ffi::OsStr::new("GIT_SSH_COMMAND")),
                Some(&None)
            );
            let target = crate::remote::parse(Path::new("ssh://build/home/me/app"));
            assert!(target.is_some());
            if let Some(target) = target {
                let request = remote_request(&target, &Extra::default(), &["push"]);
                assert!(
                    !request.env.iter().any(|(key, _)| key == "GIT_CONFIG_GLOBAL"
                        || key == "GH_TOKEN"
                        || key == "GITLAB_TOKEN")
                );
                assert!(!request.script.contains("/tmp/work key"));
            }
        });
    }
}
