//! Running commands on a repository's machine, through shells kept open there (`ssh host sh`,
//! `wsl.exe -d distro --exec sh`; see [`super::pool`] and [`super::shell`]), so a command costs
//! one round trip and not a new connection (well over a second for SSH, which can't share one
//! connection between processes on Windows).
//!
//! Each kept-open transport runs an owned supervisor broker. A request is a single complete
//! JSON line; user code cannot start from a partial line. Exact nonce-bound ownership and
//! terminal-receipt frames precede the binary length header and frozen stdout/stderr. EOF,
//! deadlines and cancellation stop and reap through the same owner without opening a second
//! connection. Ambiguous delivery is never retried.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::pool::{Lane, pool};
use super::shell::Shell;
use super::{Host, sh_path, sh_quote};

/// What a shell prints once it is ready, after whatever the login printed.
pub(super) const READY: &str = "__GITCRAFT_READY__";

/// Where else git may be: a shell that isn't a login one has a short PATH.
const MORE_PATH: &str =
    "PATH=\"$PATH:$HOME/.local/bin:$HOME/bin:/usr/local/bin:/opt/homebrew/bin\"; export PATH";

/// What a command's error output starts with when its folder isn't there.
const NO_FOLDER: &str = "gitcraft: no such folder: ";

/// The largest output a header is believed for (a server that says more is broken).
const MAX_OUTPUT: usize = 64 << 20;

/// A command to run on another machine.
#[derive(Clone, Default)]
pub struct Request {
    /// The folder to run in, as a path there.
    pub cwd: Option<String>,
    /// What to run, as `sh` reads it.
    pub script: String,
    /// Variables set for it (never in a command line: a password stays out of `ps`). Names
    /// that aren't a shell variable's are left out.
    pub env: Vec<(String, String)>,
    /// Variables it must not inherit.
    pub unset: Vec<String>,
    /// What it reads on its input; nothing (`/dev/null`) when `None`.
    pub stdin: Option<Vec<u8>>,
    /// It goes over the network and may take minutes (fetch, push…): run in shells of their
    /// own, so the quick reads aren't left waiting behind it.
    pub long: bool,
    /// A test run of a parallel bisect (which may take as long as its own limit): run in shells
    /// of their own, as many side by side as there are tests at once.
    pub test: bool,
    /// How long it may take before it is stopped; `None`: as long as its lane allows (see
    /// [`Request::time_limit`]).
    pub timeout: Option<std::time::Duration>,
    /// It isn't git and may take minutes (an AI tool): run in shells apart from git's, quick
    /// or long, so neither waits behind it.
    pub apart: bool,
}

impl Request {
    /// What it runs, in short, for a message: its script's first line.
    fn what(&self) -> String {
        let first = self.script.lines().next().unwrap_or_default();
        // `exec git …`, or `: git …` before a script that runs it (see `git::hooks`).
        let first = (first
            .strip_prefix("exec ")
            .or_else(|| first.strip_prefix(": ")))
        .unwrap_or(first);
        first.chars().take(80).collect()
    }

    /// The shells it runs in. One allowed to run longer than any network operation is a
    /// program the user works in (a diff or merge tool): a lane of its own, so that it never
    /// holds up a fetch or a push.
    pub(super) fn lane(&self) -> Lane {
        match self.timeout {
            _ if self.test => Lane::Test,
            _ if self.apart => Lane::Apart,
            Some(limit) if limit > LONG_FOR => Lane::Tool,
            _ if self.long => Lane::Long,
            _ => Lane::Quick,
        }
    }

    /// How long it may run before it is stopped.
    pub(super) fn time_limit(&self) -> Duration {
        self.timeout.unwrap_or(if self.long || self.test {
            LONG_FOR
        } else {
            QUICK_FOR
        })
    }
}

/// How long a quick read may take at most (a huge repository's first `status` can take
/// minutes; one that hangs is stopped after this).
const QUICK_FOR: Duration = Duration::from_secs(300);

/// How long a network operation may take at most.
pub const LONG_FOR: Duration = Duration::from_secs(1800);

/// Without the variables' values (a password) nor the input.
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.env.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("Request")
            .field("cwd", &self.cwd)
            .field("script", &self.script)
            .field("env", &names)
            .field("unset", &self.unset)
            .field("stdin", &self.stdin.as_ref().map(Vec::len))
            .field("long", &self.long)
            .field("test", &self.test)
            .field("timeout", &self.timeout)
            .field("apart", &self.apart)
            .finish()
    }
}

/// How a command ended, and what it printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs `req` on `host`, in one of the shells kept open there.
pub fn exec(host: &Host, req: &Request) -> Result<Output, String> {
    exec_inner(host, req, false)
}

fn exec_inner(host: &Host, req: &Request, _cleanup: bool) -> Result<Output, String> {
    super::ui_guard::off_ui_thread(&req.what())?;
    let lane = req.lane();
    let pool = pool(host, lane);
    let mut taken = pool.take(host, lane)?;
    let nonce = nonce();
    let script = framed(req, &nonce)?;
    let shell = taken.shell().ok_or("remote shell unavailable")?;
    let limit = req.time_limit();
    match shell.run(&script, &nonce, Instant::now() + limit) {
        Ok(out) => {
            if super::transport::reusable(host) {
                taken.give_back();
            }
            Ok(out)
        }
        Err(failure) => Err(format!(
            "{} on {}: {}",
            req.what(),
            host.label(),
            failure.message()
        )),
    }
}

/// Runs `req` on `host` only in a shell already open and idle there: `None`, without
/// connecting, when there is none. For reads nobody asked for (the start page's look at a
/// recent repository), which must never ask for a password nor start WSL.
pub fn exec_if_connected(host: &Host, req: &Request) -> Option<Result<Output, String>> {
    exec_if_connected_inner(host, req, false)
}

fn exec_if_connected_inner(
    host: &Host,
    req: &Request,
    _cleanup: bool,
) -> Option<Result<Output, String>> {
    if let Err(e) = super::ui_guard::off_ui_thread(&req.what()) {
        return Some(Err(e));
    }
    if !super::transport::reusable(host) {
        return None;
    }
    let pool = pool(host, Lane::Quick);
    let mut taken = pool.take_idle(UNASKED_FRESH)?;
    let nonce = nonce();
    let script = match framed(req, &nonce) {
        Ok(script) => script,
        Err(e) => return Some(Err(e)),
    };
    let limit = req.time_limit();
    let ran = taken.shell()?.run(&script, &nonce, Instant::now() + limit);
    Some(match ran {
        Ok(out) => {
            taken.give_back();
            Ok(out)
        }
        Err(failure) => Err(format!(
            "{} didn't answer in time: {}",
            req.what(),
            failure.message()
        )),
    })
}

/// How recently a shell must have been used by something asked for, for
/// [`exec_if_connected`] to use it: a connection only those reads would use is left to close.
const UNASKED_FRESH: Duration = Duration::from_secs(240);

static CLEANUPS: (std::sync::Mutex<usize>, std::sync::Condvar) =
    (std::sync::Mutex::new(0), std::sync::Condvar::new());

struct Cleanup;
impl Cleanup {
    fn new() -> Self {
        *CLEANUPS
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        Self
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        *CLEANUPS
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) -= 1;
        CLEANUPS.1.notify_all();
    }
}
pub(super) fn wait_for_cleanup() {
    let mut active = CLEANUPS
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while *active != 0 {
        active = CLEANUPS
            .1
            .wait(active)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

/// Prepared standalone stream for the shared asynchronous process service. Persistent
/// shells and their protocol are deliberately unchanged.
pub(crate) struct Streaming {
    pub command: Command,
    pub input: Vec<u8>,
    pub ready: &'static str,
    pub connected: Option<Box<dyn FnOnce() + Send>>,
    pub lease: super::supervisor::Lease,
    nonce: String,
    host: Host,
    durable: bool,
}
impl Streaming {
    pub(crate) fn configure(self, options: &mut crate::processes::Options) -> Command {
        options.input = Some(self.input);
        options.hold_input = true;
        options.stdout_ready = Some(self.ready);
        options.ready = self.connected;
        let mut marker = super::supervisor::Marker::new(self.nonce, self.lease.clone());
        options.stderr_filter = Some(Box::new(move |bytes| marker.feed(bytes)));
        let ready = self.lease.clone();
        options.stop_ready = Some(Box::new(move || ready.endpoint().is_some()));
        let lease = self.lease.clone();
        let host = self.host.clone();
        options.complete_remote = Some(Box::new(move || {
            let endpoint = lease
                .endpoint()
                .ok_or("missing output completion endpoint")?;
            confirm_delivery(&host, &endpoint)
        }));
        if !self.durable {
            let lease = self.lease.clone();
            let host = self.host.clone();
            options.retire_remote = Some(Box::new(move || {
                let endpoint = lease.endpoint().ok_or("no endpoint to retire")?;
                retire_endpoint(&host, &endpoint)
            }));
        }
        let lease = self.lease;
        let host = self.host;
        options.stop_remote = Some(Box::new(move || {
            let endpoint = lease
                .endpoint()
                .ok_or("no ownership handshake; cleanup remains unknown")?;
            stop_endpoint(&host, &endpoint)
        }));
        self.command
    }
}

pub(crate) fn streaming_command(host: &Host, req: &Request) -> Result<Streaming, String> {
    streaming_command_record(host, req, None)
}

pub(crate) fn streaming_command_record(
    host: &Host,
    req: &Request,
    record: Option<&str>,
) -> Result<Streaming, String> {
    streaming_command_authority(host, req, record, None, false)
}
pub(crate) fn streaming_command_authority(
    host: &Host,
    req: &Request,
    record: Option<&str>,
    workspace: Option<&super::workspace_authority::WorkspaceAuthority>,
    cleanup: bool,
) -> Result<Streaming, String> {
    streaming_command_bounded(host, req, record, workspace, cleanup, 64 << 20)
}
pub(crate) fn streaming_command_bounded(
    host: &Host,
    req: &Request,
    record: Option<&str>,
    workspace: Option<&super::workspace_authority::WorkspaceAuthority>,
    cleanup: bool,
    output_limit: usize,
) -> Result<Streaming, String> {
    super::ui_guard::off_ui_thread(&req.what())?;
    let (command, attempt) = shell_command(host, false);
    let nonce = nonce();
    let input = format!(
        "{MORE_PATH}\n{}",
        super::supervisor::streaming_bounded(
            req,
            &nonce,
            record,
            workspace,
            cleanup,
            output_limit
        )?
    )
    .into_bytes();
    Ok(Streaming {
        command,
        input,
        ready: READY,
        connected: attempt.map(|a| Box::new(move || a.connected()) as Box<dyn FnOnce() + Send>),
        lease: Default::default(),
        nonce,
        host: host.clone(),
        durable: record.is_some(),
    })
}

type ClaimCommand = (Command, Vec<u8>, Option<Box<dyn FnOnce() + Send>>);

pub(super) fn claim_command(host: &Host, data: &str) -> ClaimCommand {
    let (command, connection) = shell_command(host, false);
    let input = format!("{MORE_PATH}\n{}", super::supervisor::claim(data)).into_bytes();
    (
        command,
        input,
        connection.map(|c| Box::new(move || c.connected()) as Box<dyn FnOnce() + Send>),
    )
}

pub(crate) fn stop_endpoint(
    host: &Host,
    endpoint: &super::supervisor::Endpoint,
) -> Result<(), String> {
    let _cleanup = Cleanup::new();
    let (command, _connection) = shell_command(host, false);
    let out = crate::processes::run_cleanup(
        command,
        crate::processes::Options {
            input: Some(endpoint.control_script()?.into_bytes()),
            timeout: Some(Duration::from_secs(10)),
            output_limit: 4096,
            stderr_limit: Some(4096),
            ..Default::default()
        },
    )?
    .complete()?;
    if out.status.success() && endpoint.confirms(&out.stdout) {
        Ok(())
    } else {
        Err("the supervisor did not confirm its atomic stop/reap receipt".into())
    }
}

pub(crate) fn retire_endpoint(
    host: &Host,
    endpoint: &super::supervisor::Endpoint,
) -> Result<(), String> {
    let _cleanup = Cleanup::new();
    let (command, _connection) = shell_command(host, false);
    let out = crate::processes::run_cleanup(
        command,
        crate::processes::Options {
            input: Some(endpoint.retire_script()?.into_bytes()),
            timeout: Some(Duration::from_secs(10)),
            output_limit: 4096,
            stderr_limit: Some(4096),
            ..Default::default()
        },
    )?
    .complete()?;
    if out.status.success() && out.stdout.trim_ascii() == b"retired" {
        Ok(())
    } else {
        Err("private endpoint evidence retained".into())
    }
}

fn confirm_delivery(host: &Host, endpoint: &super::supervisor::Endpoint) -> Result<(), String> {
    let _cleanup = Cleanup::new();
    let (command, _connection) = shell_command(host, false);
    let out = crate::processes::run_cleanup(
        command,
        crate::processes::Options {
            input: Some(endpoint.control_script()?.into_bytes()),
            timeout: Some(Duration::from_secs(10)),
            output_limit: 4096,
            stderr_limit: Some(4096),
            ..Default::default()
        },
    )?
    .complete()?;
    if out.status.success() && endpoint.complete(&out.stdout) {
        Ok(())
    } else {
        Err("owned command stopped but output delivery is incomplete or unconfirmed".into())
    }
}

/// A word that names a shell variable (`GIT_EDITOR`), and nothing else.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether a command's error output says its folder isn't there.
pub fn is_missing_folder(stderr: &[u8]) -> bool {
    stderr.starts_with(NO_FOLDER.as_bytes())
}

/// `req` as one `sh` command: in a subshell, so its folder and variables stay its own, with
/// `redirects` applied to it.
pub(super) fn inline(req: &Request, redirects: &str) -> String {
    let mut s = String::from("(");
    if let Some(cwd) = &req.cwd {
        let path = sh_path(cwd);
        s.push_str(&format!(
            "cd -- {path} 2>/dev/null || {{ printf '{NO_FOLDER}%s\\n' {path} >&2; exit 1; }}\n"
        ));
    }
    for name in req.unset.iter().filter(|n| is_variable_name(n)) {
        s.push_str(&format!("unset {name}\n"));
    }
    for (name, value) in req.env.iter().filter(|(n, _)| is_variable_name(n)) {
        s.push_str(&format!("{name}={}; export {name}\n", sh_quote(value)));
    }
    s.push_str(&req.script);
    s.push_str(&format!("\n) {redirects}"));
    s
}

#[cfg(test)]
mod tests;

mod connection;
#[cfg(all(test, unix))]
pub use connection::LOOPBACK;
use connection::*;
pub(super) use connection::{setup_script, start_shell};

mod framing;
pub(super) use framing::{framed, nonce, parse_header, parse_ready};
