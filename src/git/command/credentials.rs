//! Credential lookup and unattended authentication behavior.

use super::*;

/// A username and password (or token) for an HTTPS remote, typed into gitcraft.
#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    /// None is legacy/unbound state and is refused by all transient helpers.
    pub scope: Option<super::super::CredentialScope>,
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
pub(super) fn unattended_extra(unix: bool) -> Extra {
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
pub(super) fn unattended(cmd: &mut Command) {
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

/// What git's credential helpers keep for `url` (`https://host`), asking nobody: no terminal
/// prompt, no askpass program, no sign-in window.
pub fn stored_credentials(url: &str) -> Option<Credentials> {
    let _timer = super::super::backend::CliTimer::new(&["credential"]);
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
pub(super) fn parse_credentials(out: &str) -> Option<Credentials> {
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
