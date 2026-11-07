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
    super::ui_guard::off_ui_thread(&req.what())?;
    let (command, attempt) = shell_command(host, false);
    let nonce = nonce();
    let input = format!(
        "{MORE_PATH}\n{}",
        super::supervisor::streaming(req, &nonce, record)?
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

/// For the tests: the SSH host whose shells are `sh` on this machine, so that everything
/// sent to another machine (the framing, the input, the pools) runs as it does over SSH.
#[cfg(all(test, unix))]
pub const LOOPBACK: &str = "gitcraft-loopback";

/// A new shell kept open on `host`.
pub(super) fn start_shell(host: &Host) -> Result<Shell, String> {
    let (cmd, attempt) = shell_command(host, true);
    let shell = Shell::start(cmd, &host.label())?;
    // Signed in: a password to remember is kept now (a failed start drops it).
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    Ok(shell)
}

/// What a new shell is sent first: its temporary folder (removed when the shell ends, also on
/// a hang-up or a signal), its PATH, then [`READY`] and its process (see [`parse_ready`]).
pub(super) fn setup_script() -> String {
    format!("{MORE_PATH}\n{}", super::supervisor::setup())
}

/// The process that runs `sh` on `host`, reading its script from its input, and (over SSH)
/// the attempt to tell once it has signed in.
struct Connection {
    attempt: Option<super::askpass::Attempt>,
    _opening: Option<super::transport::Opening>,
}

impl Connection {
    fn connected(self) {
        if let Some(attempt) = self.attempt {
            attempt.connected();
        }
    }
}

fn shell_command(host: &Host, serialize: bool) -> (Command, Option<Connection>) {
    #[cfg(all(test, unix))]
    if *host == Host::Ssh(LOOPBACK.to_owned()) {
        return (Command::new("sh"), None);
    }
    let mut connection = None;
    // Changed further only on Windows (no console window).
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = match host {
        Host::Ssh(dest) => {
            let mut cmd = Command::new("ssh");
            // No terminal, no `~.` escapes: the stream is the protocol.
            cmd.args(["-T", "-e", "none"]);
            // Its questions (a password, a passphrase, a new host key) asked in the window
            // (see `askpass`); where that can't be, none asked: it fails, and says why.
            let mut attempt = None;
            match super::askpass::env_for(&host.label()) {
                Some((env, started)) => {
                    cmd.envs(env);
                    attempt = Some(started);
                }
                None => {
                    cmd.args(["-o", "BatchMode=yes"]);
                }
            }
            let opening = super::transport::configure(&mut cmd, dest, serialize);
            // The Unix wrapper includes the original configuration before its defaults.
            // Unsupported platforms / inaccessible private storage keep the old behavior.
            if opening.is_none()
                && let Some(config) =
                    std::env::var_os("GITCRAFT_SSH_CONFIG").filter(|c| !c.is_empty())
            {
                cmd.arg("-F").arg(config);
            }
            connection = Some(Connection {
                attempt,
                _opening: opening,
            });
            cmd.args([
                "-o",
                "ConnectTimeout=15",
                // A dropped network ends the shell (and its waiting command) within ~45 s.
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "--",
            ]);
            cmd.arg(super::ssh_destination(dest));
            cmd.arg("sh");
            cmd
        }
        Host::Wsl(distro) => {
            let mut cmd = Command::new("wsl.exe");
            cmd.args(["-d", distro, "--exec", "sh"]);
            cmd
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    (cmd, connection)
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

/// A word no command's output will hold by chance: new for every command.
pub(super) fn nonce() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// `req` as the script a kept-open shell is sent (see the module's doc). The outputs are
/// copied before they are measured: something the command left running (a hook's background
/// job) may still be writing to them, and the bytes sent must be the bytes the header counts.
/// What can't be passed on or kept (a full disk) fails the command (125) and says why.
pub(super) fn framed(req: &Request, nonce: &str) -> Result<String, String> {
    super::supervisor::request(req, nonce, None)
}

pub(super) fn parse_ready(line: &[u8]) -> bool {
    line.trim_ascii_end() == READY.as_bytes()
}

/// `R <nonce> <code> <stdout bytes> <stderr bytes>` (`wc -c` may pad the numbers with
/// spaces), for the command of `nonce` only.
pub(super) fn parse_header(line: &str, nonce: &str) -> Option<(i32, usize, usize)> {
    let mut words = line.split_whitespace();
    if words.next()? != "R" || words.next()? != nonce {
        return None;
    }
    let code = words.next()?.parse().ok()?;
    let out: usize = words.next()?.parse().ok()?;
    let err: usize = words.next()?.parse().ok()?;
    (words.next().is_none() && out <= MAX_OUTPUT && err <= MAX_OUTPUT).then_some((code, out, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A diff or merge tool, allowed longer than any network operation, never takes a shell
    /// fetch and push need; and, the lane full, a further one is refused at once.
    #[test]
    fn a_tool_runs_in_a_lane_of_its_own() {
        let req = |long, timeout| Request {
            long,
            timeout,
            ..Request::default()
        };
        assert_eq!(req(false, None).lane(), Lane::Quick);
        assert_eq!(req(true, None).lane(), Lane::Long);
        assert_eq!(req(true, Some(LONG_FOR)).lane(), Lane::Long);
        assert_eq!(req(true, Some(LONG_FOR * 24)).lane(), Lane::Tool);
        let busy = Lane::Tool.busy(&Host::Ssh("build".to_owned()));
        assert_eq!(
            busy,
            "4 diff or merge tools are already open on SSH · build: close one of them first"
        );
    }

    #[test]
    fn nothing_runs_without_an_open_connection() {
        let host = Host::Ssh("gitcraft-never-connected.invalid".to_owned());
        let req = Request {
            script: "true".to_owned(),
            ..Request::default()
        };
        assert!(exec_if_connected(&host, &req).is_none());
    }

    /// Reads nobody asked for run in a shell already open, don't count as its use (so the
    /// connection still closes when idle), and one that times out never opens another
    /// connection, not even to stop what hung.
    #[cfg(unix)]
    #[test]
    fn unasked_reads_never_connect_nor_keep_a_connection_alive() -> Result<(), String> {
        let host = Host::Ssh("gitcraft-unasked-test.invalid".to_owned());
        let pool = pool(&host, Lane::Quick);
        pool.add_idle(Shell::start(Command::new("sh"), "test")?);
        let (_, _, before) = pool.looks();
        std::thread::sleep(Duration::from_millis(20));
        let quick = Request {
            script: "printf ok".to_owned(),
            ..Request::default()
        };
        let out = exec_if_connected(&host, &quick).ok_or("no shell taken")??;
        assert_eq!(out.stdout, b"ok");
        let (open, failed, after) = pool.looks();
        assert_eq!((open, failed), (1, false));
        assert_eq!(after, before, "the read counted as use");
        // Hangs past its limit: the shell is closed, and nothing connects.
        let slow = Request {
            script: "sleep 5".to_owned(),
            timeout: Some(Duration::from_millis(300)),
            ..Request::default()
        };
        let timed_out = exec_if_connected(&host, &slow).ok_or("no shell taken")?;
        assert!(timed_out.is_err_and(|e| e.contains("didn't answer in time")));
        std::thread::sleep(Duration::from_millis(300));
        let (open, failed, _) = pool.looks();
        assert_eq!((open, failed), (0, false), "a connection was tried");
        assert!(exec_if_connected(&host, &quick).is_none());
        Ok(())
    }

    #[test]
    fn headers_read_with_padded_numbers_for_their_command_only() {
        assert_eq!(parse_header("R ab1 0 12 0\n", "ab1"), Some((0, 12, 0)));
        assert_eq!(
            parse_header("R ab1 128      7       3\n", "ab1"),
            Some((128, 7, 3))
        );
        assert_eq!(parse_header("R ab2 0 1 2\n", "ab1"), None);
        assert_eq!(parse_header("X ab1 0 1 2\n", "ab1"), None);
        assert_eq!(parse_header("R ab1 0 1\n", "ab1"), None);
        // A size no output has.
        assert_eq!(parse_header("R ab1 0 99999999999 0\n", "ab1"), None);
    }

    #[test]
    fn a_request_sets_its_folder_and_variables_in_a_subshell() {
        let req = Request {
            cwd: Some("~/my repo".to_owned()),
            script: "exec git status".to_owned(),
            env: vec![
                ("A".to_owned(), "it's".to_owned()),
                ("X;rm -rf ~".to_owned(), "1".to_owned()),
            ],
            unset: vec!["B".to_owned(), "$(x)".to_owned()],
            ..Request::default()
        };
        let s = inline(&req, "</dev/null");
        assert_eq!(
            s,
            "(cd -- \"$HOME\"'/my repo' 2>/dev/null || { printf 'gitcraft: no such folder: %s\\n' \"$HOME\"'/my repo' >&2; exit 1; }\nunset B\nA='it'\\''s'; export A\nexec git status\n) </dev/null"
        );
    }

    #[test]
    fn a_framed_request_has_one_complete_json_line() -> Result<(), String> {
        let request = framed(
            &Request {
                script: "echo x\necho y".into(),
                stdin: Some(vec![0, 255, b'\n']),
                ..Default::default()
            },
            "ab1",
        )?;
        assert_eq!(request.bytes().filter(|&b| b == b'\n').count(), 1);
        let value: serde_json::Value = serde_json::from_str(&request).map_err(|e| e.to_string())?;
        assert_eq!(value["nonce"], "ab1");
        assert!(
            value["script"]
                .as_str()
                .is_some_and(|s| s.contains("echo x\necho y"))
        );
        Ok(())
    }

    #[test]
    fn a_ready_supervisor_never_grants_pid_authority() {
        assert!(parse_ready(b"__GITCRAFT_READY__\n"));
        assert!(!parse_ready(b"__GITCRAFT_READY__ 4242\n"));
        assert!(!parse_ready(b"login banner\n"));
    }

    /// This production timeout must acknowledge the owner; no process-list assertion.
    #[cfg(unix)]
    #[test]
    fn stopping_ends_a_whole_tree() -> Result<(), String> {
        let effect = std::env::temp_dir().join(format!("gitcraft-owned-stop-{}", nonce()));
        let request = Request {
            script: format!(
                "sh -c {}",
                sh_quote(&format!(
                    "sleep 1; printf survived >{}",
                    sh_quote(&effect.to_string_lossy())
                ))
            ),
            timeout: Some(Duration::from_millis(200)),
            ..Default::default()
        };
        let result = exec(&Host::Ssh(LOOPBACK.into()), &request);
        assert!(result.is_err_and(|e| e.contains("confirmed stop and reap")));
        std::thread::sleep(Duration::from_secs(1));
        assert!(!effect.exists(), "a child survived cancellation");
        Ok(())
    }

    #[test]
    fn a_request_shows_no_values() {
        let req = Request {
            env: vec![("PASSWORD".to_owned(), "hunter2".to_owned())],
            ..Request::default()
        };
        let shown = format!("{req:?}");
        assert!(
            shown.contains("PASSWORD") && !shown.contains("hunter2"),
            "{shown}"
        );
    }
}
