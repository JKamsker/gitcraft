//! Shells kept open on a repository's machine (`ssh host sh`, `wsl.exe -d distro --exec sh`),
//! so a command there costs one round trip and not a new connection (well over a second for
//! SSH, which can't share one connection between processes on Windows).
//!
//! Each command goes to a shell as a short script that runs it with its output into files of
//! the shell's own temporary folder, then prints a header line (`R <nonce> <exit code> <stdout
//! bytes> <stderr bytes>`) and both outputs as they are: any bytes, any size, no escaping. The
//! nonce, new for every command, makes sure what is read answers that command and no other. A
//! few shells per machine run side by side; one that breaks, or answers anything unexpected,
//! is dropped and replaced.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use super::{Host, sh_path, sh_quote};

/// How many shells per machine at most: the app reads a repository with several commands at
/// once (status, refs, log, stashes…), more than that wait.
const MAX_SESSIONS: usize = 6;

/// After a shell couldn't be started, commands fail at once with its error for this long
/// rather than each trying (and waiting for) a connection of its own.
const FAILED_FOR: Duration = Duration::from_secs(5);

/// What a shell prints once it is ready, after whatever the login printed.
const READY: &str = "__GITCRAFT_READY__";

/// Makes the shell's temporary folder (again, if something cleared it since).
const MAKE_TEMP: &str = "T=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-sh.XXXXXX\")";

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
}

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
    let pool = pool(host);
    loop {
        let (mut session, reused) = pool.take(host)?;
        let nonce = nonce();
        match session.run(&framed(req, &nonce), &nonce) {
            Ok(out) => {
                pool.give_back(session);
                return Ok(out);
            }
            // A shell left open went away meanwhile: the script never reached it, so another
            // shell can run it.
            Err(Failure::NotSent(_)) if reused => pool.forget(session),
            Err(failure) => {
                pool.forget(session);
                return Err(failure.message());
            }
        }
    }
}

/// Starts `req` on `host` in a process of its own, whose output the caller reads as it comes
/// (a long `git log`). Its input is taken: the script went there.
pub fn spawn(host: &Host, req: &Request) -> Result<Child, String> {
    let mut cmd = shell_command(host);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| start_error(host, &e))?;
    let script = format!("{}\n", inline(req, "</dev/null"));
    let written = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(script.as_bytes()));
    if let Some(Err(e)) = written {
        let _ = child.kill();
        return Err(format!("could not reach {}: {e}", host.label()));
    }
    Ok(child)
}

/// The process that runs `sh` on `host`, reading its script from its input.
fn shell_command(host: &Host) -> Command {
    let mut cmd = match host {
        Host::Ssh(dest) => {
            let mut cmd = Command::new("ssh");
            cmd.args([
                // No terminal, no `~.` escapes: the stream is the protocol.
                "-T",
                "-e",
                "none",
                // Never a password or host key prompt nobody would see: it fails, and says why.
                "-o",
                "BatchMode=yes",
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
    cmd
}

fn start_error(host: &Host, e: &std::io::Error) -> String {
    let program = match host {
        Host::Ssh(_) => "ssh",
        Host::Wsl(_) => "wsl.exe",
    };
    format!("could not run {program} to reach {}: {e}", host.label())
}

/// A word that names a shell variable (`GIT_EDITOR`), and nothing else.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `req` as one `sh` command: in a subshell, so its folder and variables stay its own, with
/// `redirects` applied to it.
fn inline(req: &Request, redirects: &str) -> String {
    let mut s = String::from("(");
    if let Some(cwd) = &req.cwd {
        s.push_str(&format!("cd -- {} || exit 1\n", sh_path(cwd)));
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
fn nonce() -> String {
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

/// `req` as the script a kept-open shell is sent: `S <nonce>` once it starts, then the header
/// and both outputs (see the module's doc). The outputs are copied before they are measured:
/// something the command left running (a hook's background job) may still be writing to them,
/// and the bytes sent must be the bytes the header counts.
fn framed(req: &Request, nonce: &str) -> String {
    let mut s = format!("printf 'S %s\\n' {nonce}\n[ -d \"$T\" ] || {MAKE_TEMP} || exit 1\n");
    let input = match &req.stdin {
        Some(bytes) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            s.push_str(&format!("printf '%s' '{b64}' | base64 -d >\"$T/i\"\n"));
            "\"$T/i\""
        }
        None => "/dev/null",
    };
    s.push_str(&inline(req, &format!("<{input} >\"$T/o\" 2>\"$T/e\"")));
    s.push_str(&format!(
        concat!(
            "\nr=$?\n",
            "cat \"$T/o\" >\"$T/O\"; cat \"$T/e\" >\"$T/E\"\n",
            "printf 'R %s %s %s %s\\n' {nonce} \"$r\" \"$(wc -c <\"$T/O\")\" \"$(wc -c <\"$T/E\")\"\n",
            "cat \"$T/O\" \"$T/E\"\n",
            "rm -f \"$T/i\" \"$T/o\" \"$T/e\" \"$T/O\" \"$T/E\"\n",
        ),
        nonce = nonce
    ));
    s
}

/// What went wrong running a command in a shell.
enum Failure {
    /// The script never got to the shell: it can be run in another one.
    NotSent(String),
    /// After it was sent: whether it ran, and what it did, is unknown.
    Lost(String),
}

impl Failure {
    fn message(self) -> String {
        match self {
            Failure::NotSent(m) | Failure::Lost(m) => m,
        }
    }
}

/// A shell kept open on another machine.
struct Session {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
    /// The end of what the process said on its error output (ssh's own errors).
    said: Arc<Mutex<String>>,
    label: String,
}

impl Session {
    fn start(host: &Host) -> Result<Self, String> {
        let mut cmd = shell_command(host);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| start_error(host, &e))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(format!("could not reach {}", host.label()));
        };
        let said = Arc::new(Mutex::new(String::new()));
        let drain = drain_errors(stderr, Arc::clone(&said));
        let mut session = Self {
            child,
            stdin,
            out: BufReader::new(stdout),
            said,
            label: host.label(),
        };
        match session.handshake() {
            Ok(()) => Ok(session),
            Err(e) => {
                // Its error output, read to the end once the process is gone, says why.
                let _ = session.child.kill();
                let _ = session.child.wait();
                let _ = drain.join();
                let said = session.said_text();
                Err(match said.is_empty() {
                    true => e,
                    false => format!("could not reach {}: {said}", session.label),
                })
            }
        }
    }

    /// Sets up the shell's temporary folder (removed when the shell ends, also on a hang-up
    /// or a signal) and waits for it to say it's ready.
    fn handshake(&mut self) -> Result<(), String> {
        let setup = format!(
            concat!(
                "{make} || exit 1\n",
                // Where else git may be: a shell that isn't a login one has a short PATH.
                "PATH=\"$PATH:$HOME/.local/bin:$HOME/bin:/usr/local/bin:/opt/homebrew/bin\"\n",
                "export PATH\n",
                "trap 'rm -rf \"$T\"' EXIT\n",
                "trap 'rm -rf \"$T\"; exit 1' HUP INT TERM PIPE\n",
                "printf '\\n{ready}\\n'\n",
            ),
            make = MAKE_TEMP,
            ready = READY
        );
        let lost = |e: std::io::Error| format!("could not reach {}: {e}", self.label);
        self.stdin.write_all(setup.as_bytes()).map_err(lost)?;
        self.stdin.flush().map_err(lost)?;
        // What a login script printed comes first, in whatever encoding.
        let mut line = Vec::new();
        loop {
            line.clear();
            let n = self.out.read_until(b'\n', &mut line).map_err(lost)?;
            if n == 0 {
                return Err(format!(
                    "could not reach {}: the connection closed",
                    self.label
                ));
            }
            if line.trim_ascii_end() == READY.as_bytes() {
                return Ok(());
            }
        }
    }

    /// Whether its process has ended (the connection dropped while it waited).
    fn ended(&mut self) -> bool {
        self.child
            .try_wait()
            .map_or(true, |status| status.is_some())
    }

    fn run(&mut self, script: &str, nonce: &str) -> Result<Output, Failure> {
        self.stdin
            .write_all(script.as_bytes())
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| Failure::NotSent(self.lost(&e.to_string())))?;
        // From here on, the command may have run: nothing is run again.
        let lost = |s: &Self, why: &str| Failure::Lost(s.lost(why));
        let mut line = String::new();
        match self.out.read_line(&mut line) {
            Ok(_) if line.trim_end() == format!("S {nonce}") => {}
            Ok(0) => return Err(lost(self, "the connection closed")),
            Ok(_) => return Err(lost(self, "it answered out of turn")),
            Err(e) => return Err(lost(self, &e.to_string())),
        }
        line.clear();
        let header = match self.out.read_line(&mut line) {
            Ok(n) if n > 0 => parse_header(&line, nonce),
            Ok(_) => None,
            Err(e) => return Err(lost(self, &e.to_string())),
        };
        let Some((code, out_len, err_len)) = header else {
            return Err(lost(self, "the connection closed while a command ran"));
        };
        let stdout = self.read_exactly(out_len).map_err(|e| lost(self, &e))?;
        let stderr = self.read_exactly(err_len).map_err(|e| lost(self, &e))?;
        Ok(Output {
            code,
            stdout,
            stderr,
        })
    }

    /// The next `len` bytes, held only as they arrive: a header that says more than comes
    /// doesn't make the app set aside memory for it.
    fn read_exactly(&mut self, len: usize) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        (&mut self.out)
            .take(len as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() == len {
            Ok(bytes)
        } else {
            Err("the connection closed while a command's output came".to_owned())
        }
    }

    /// The error for a shell that stopped answering, with what ssh said if anything.
    fn lost(&self, why: &str) -> String {
        let said = self.said_text();
        if said.is_empty() {
            format!("lost the connection to {}: {why}", self.label)
        } else {
            format!("lost the connection to {}: {said}", self.label)
        }
    }

    fn said_text(&self) -> String {
        lock(&self.said).trim().to_owned()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Its shell ends (and clears its folder) as the connection goes.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `R <nonce> <code> <stdout bytes> <stderr bytes>` (`wc -c` may pad the numbers with
/// spaces), for the command of `nonce` only.
fn parse_header(line: &str, nonce: &str) -> Option<(i32, usize, usize)> {
    let mut words = line.split_whitespace();
    if words.next()? != "R" || words.next()? != nonce {
        return None;
    }
    let code = words.next()?.parse().ok()?;
    let out = words.next()?.parse().ok()?;
    let err = words.next()?.parse().ok()?;
    Some((code, out, err))
}

/// Keeps reading the process's error output (unread, a full pipe would stop it), keeping the
/// last few lines.
fn drain_errors(
    mut stderr: std::process::ChildStderr,
    said: Arc<Mutex<String>>,
) -> std::thread::JoinHandle<()> {
    const KEEP: usize = 2048;
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut text = lock(&said);
            // `wsl.exe` says its own errors in UTF-16: read as text, every other byte is a NUL.
            text.extend(
                String::from_utf8_lossy(&buf[..n])
                    .chars()
                    .filter(|&c| c != '\0'),
            );
            if text.len() > KEEP {
                let cut = text.len() - KEEP;
                let cut = (cut..text.len())
                    .find(|&i| text.is_char_boundary(i))
                    .unwrap_or(0);
                text.drain(..cut);
            }
        }
    })
}

/// The shells of one machine.
#[derive(Default)]
struct Pool {
    state: Mutex<PoolState>,
    freed: Condvar,
}

#[derive(Default)]
struct PoolState {
    idle: Vec<Session>,
    /// Shells open or being opened, idle or not.
    open: usize,
    /// The last time one couldn't be opened, and why.
    failed: Option<(Instant, String)>,
}

impl Pool {
    /// A shell to run a command in, and whether it ran others before: an idle one still
    /// connected, else a new one when there are fewer than [`MAX_SESSIONS`], else the next one
    /// freed.
    fn take(&self, host: &Host) -> Result<(Session, bool), String> {
        let mut state = lock(&self.state);
        loop {
            while let Some(mut session) = state.idle.pop() {
                if !session.ended() {
                    return Ok((session, true));
                }
                state.open = state.open.saturating_sub(1);
            }
            // Fast only with none open: with some, the next one freed will do.
            if let Some((at, why)) = &state.failed
                && at.elapsed() < FAILED_FOR
                && state.open == 0
            {
                return Err(why.clone());
            }
            if state.open < MAX_SESSIONS {
                state.open += 1;
                drop(state);
                let started = Session::start(host);
                let mut state = lock(&self.state);
                return match started {
                    Ok(session) => {
                        state.failed = None;
                        Ok((session, false))
                    }
                    Err(why) => {
                        state.open -= 1;
                        state.failed = Some((Instant::now(), why.clone()));
                        self.freed.notify_all();
                        Err(why)
                    }
                };
            }
            state = self
                .freed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn give_back(&self, session: Session) {
        lock(&self.state).idle.push(session);
        self.freed.notify_one();
    }

    fn forget(&self, session: Session) {
        drop(session);
        let mut state = lock(&self.state);
        state.open = state.open.saturating_sub(1);
        self.freed.notify_one();
    }
}

fn pool(host: &Host) -> Arc<Pool> {
    static POOLS: OnceLock<Mutex<HashMap<Host, Arc<Pool>>>> = OnceLock::new();
    let pools = POOLS.get_or_init(Mutex::default);
    Arc::clone(lock(pools).entry(host.clone()).or_default())
}

/// A lock, also when a thread panicked holding it: what it guards stays usable.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            stdin: None,
        };
        let s = inline(&req, "</dev/null");
        assert_eq!(
            s,
            "(cd -- \"$HOME\"'/my repo' || exit 1\nunset B\nA='it'\\''s'; export A\nexec git status\n) </dev/null"
        );
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

    /// A session in a real `sh`, as a kept-open shell runs.
    #[cfg(unix)]
    fn local_session() -> Result<Session, String> {
        let mut child = Command::new("sh")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("no pipes".to_owned());
        };
        let mut session = Session {
            child,
            stdin,
            out: BufReader::new(stdout),
            said: Arc::new(Mutex::new(String::new())),
            label: "test".to_owned(),
        };
        session.handshake()?;
        Ok(session)
    }

    #[cfg(unix)]
    fn run_in(session: &mut Session, req: &Request) -> Result<Output, String> {
        let nonce = nonce();
        session
            .run(&framed(req, &nonce), &nonce)
            .map_err(Failure::message)
    }

    /// The framing runs as it is in a real `sh`, binary output and input included.
    #[cfg(unix)]
    #[test]
    fn a_framed_request_runs_in_sh() -> Result<(), String> {
        let mut session = local_session()?;
        let req = Request {
            script: "cat; printf 'err' >&2; exit 3".to_owned(),
            stdin: Some(vec![0, 1, 2, b'\n', 255]),
            ..Request::default()
        };
        let out = run_in(&mut session, &req)?;
        assert_eq!(out.code, 3);
        assert_eq!(out.stdout, vec![0, 1, 2, b'\n', 255]);
        assert_eq!(out.stderr, b"err");
        // And again, on the same shell.
        let out = run_in(
            &mut session,
            &Request {
                script: "printf hi".to_owned(),
                ..Request::default()
            },
        )?;
        assert_eq!((out.code, out.stdout), (0, b"hi".to_vec()));
        Ok(())
    }

    /// Something a command leaves writing to its output after it ends doesn't spill into the
    /// next command's answer.
    #[cfg(unix)]
    #[test]
    fn output_written_late_stays_out_of_the_next_answer() -> Result<(), String> {
        let mut session = local_session()?;
        let late = Request {
            script: "printf a; (sleep 0.2; printf bbbbbbbb) &".to_owned(),
            ..Request::default()
        };
        let out = run_in(&mut session, &late)?;
        assert!(out.stdout.starts_with(b"a"));
        std::thread::sleep(Duration::from_millis(400));
        let next = run_in(
            &mut session,
            &Request {
                script: "printf next".to_owned(),
                ..Request::default()
            },
        )?;
        assert_eq!(next.stdout, b"next");
        Ok(())
    }
}
