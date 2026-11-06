//! One shell kept open on another machine, its output read by a thread of its own, so that
//! the few reads that must not wait for good (the connection being made, the check of a shell
//! left idle) can have a deadline.

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::session::{Output, nonce, parse_header, parse_job, parse_ready, setup_script};

/// How long the connection may take to be made and signed in to.
const CONNECT_FOR: Duration = Duration::from_secs(30);

/// A shell idle longer than this is asked whether it's still there before it's used: after a
/// sleep or a dropped network, a write still goes into the pipe and the command would then
/// wait for the connection to be found dead (a minute or so).
const CHECK_AFTER: Duration = Duration::from_secs(20);

/// How long that check may take.
const CHECK_FOR: Duration = Duration::from_secs(5);

/// How long the error output of a process that failed to connect is read for, at most.
pub(super) const SAID_WITHIN: Duration = Duration::from_secs(2);

/// What went wrong running a command in a shell.
pub(super) enum Failure {
    /// The script never got to the shell: it can be run in another one.
    NotSent(String),
    /// After it was sent: whether it ran, and what it did, is unknown.
    Lost(String),
    /// It didn't end by its deadline; its process there, when known, is to be stopped.
    TimedOut(Option<u32>),
}

impl Failure {
    pub(super) fn message(self) -> String {
        match self {
            Failure::NotSent(m) | Failure::Lost(m) => m,
            Failure::TimedOut(_) => "it took too long and was stopped".to_owned(),
        }
    }
}

enum ReadError {
    Closed,
    TimedOut,
}

pub(super) struct Shell {
    child: Child,
    stdin: ChildStdin,
    /// What the shell printed, as the reader thread got it.
    chunks: Receiver<Vec<u8>>,
    /// Read and not taken yet.
    pending: Vec<u8>,
    /// The end of what the process said on its error output (ssh's own errors).
    said: Arc<Mutex<String>>,
    label: String,
    /// When it last answered.
    pub(super) used: Instant,
    /// Its process there, as it said when ready: stopped, with what runs in it, when a command
    /// hangs before saying its own (closing the connection doesn't stop what runs there).
    remote_pid: Option<u32>,
}

impl Shell {
    /// Starts `cmd` (a shell on the machine `label` names), waits for it to be ready.
    pub(super) fn start(mut cmd: Command, label: &str) -> Result<Self, String> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not reach {label}: {e}"))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("could not reach {label}"));
        };
        let said = Arc::new(Mutex::new(String::new()));
        let drain = drain_errors(stderr, Arc::clone(&said));
        let mut shell = Self {
            child,
            stdin,
            chunks: read_output(stdout),
            pending: Vec::new(),
            said,
            label: label.to_owned(),
            used: Instant::now(),
            remote_pid: None,
        };
        match shell.handshake() {
            Ok(()) => {
                // What ssh said while connecting (a host key added…) says nothing about a
                // connection lost later.
                lock(&shell.said).clear();
                Ok(shell)
            }
            Err(e) => {
                let _ = shell.child.kill();
                let _ = shell.child.wait();
                // Its error output says why, once read to the end: not waited for long, as a
                // proxy it started can keep it open.
                wait_for(&drain, SAID_WITHIN);
                let said = shell.said_text();
                Err(if said.is_empty() {
                    e
                } else {
                    format!("could not reach {label}: {said}{}", connect_hint(&said))
                })
            }
        }
    }

    /// Sets the shell up and waits for it to say it's ready, after whatever the login printed.
    fn handshake(&mut self) -> Result<(), String> {
        let label = self.label.clone();
        let lost = |e: &dyn std::fmt::Display| format!("could not reach {label}: {e}");
        self.stdin
            .write_all(setup_script().as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|e| lost(&e))?;
        let mut deadline = Instant::now() + CONNECT_FOR;
        loop {
            match self.read_line(Some(deadline)) {
                Ok(line) => {
                    if let Some(pid) = parse_ready(&line) {
                        self.remote_pid = pid;
                        return Ok(());
                    }
                }
                Err(ReadError::Closed) => return Err(lost(&"the connection closed")),
                // The user is being asked something (a password): their time, not the network's.
                Err(ReadError::TimedOut) if super::askpass::asked_lately(CONNECT_FOR) => {
                    deadline = Instant::now() + CONNECT_FOR;
                }
                Err(ReadError::TimedOut) => return Err(lost(&"it didn't answer in time")),
            }
        }
    }

    /// Whether it can take a command: its process is running and, after a while idle, it
    /// answers a check in time.
    pub(super) fn usable(&mut self) -> bool {
        if self
            .child
            .try_wait()
            .map_or(true, |status| status.is_some())
        {
            return false;
        }
        if self.used.elapsed() < CHECK_AFTER {
            return true;
        }
        let nonce = nonce();
        let check = format!("printf 'P %s\\n' {nonce}\n");
        if self
            .stdin
            .write_all(check.as_bytes())
            .and_then(|()| self.stdin.flush())
            .is_err()
        {
            return false;
        }
        let answered = self
            .read_line(Some(Instant::now() + CHECK_FOR))
            .is_ok_and(|line| line.trim_ascii_end() == format!("P {nonce}").as_bytes());
        if answered {
            self.used = Instant::now();
        }
        answered
    }

    /// Runs `script` (see `session::framed`), the command of `nonce`, which must be done by
    /// `deadline`. Every read has that deadline: one past it stops what runs there, the
    /// command's own process when it said it, else the shell's (with all it started).
    pub(super) fn run(
        &mut self,
        script: &str,
        nonce: &str,
        deadline: Instant,
    ) -> Result<Output, Failure> {
        lock(&self.said).clear();
        self.stdin
            .write_all(script.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|e| Failure::NotSent(self.lost(&e.to_string())))?;
        // From here on, the command may have run: nothing is run again.
        let lost = |s: &Self, why: &str| Failure::Lost(s.lost(why));
        let closed = |s: &Self| lost(s, "the connection closed while a command ran");
        let shell_pid = self.remote_pid;
        match self.read_line(Some(deadline)) {
            Ok(line) if line.trim_ascii_end() == format!("S {nonce}").as_bytes() => {}
            Ok(_) => return Err(lost(self, "it answered out of turn")),
            Err(ReadError::TimedOut) => return Err(Failure::TimedOut(shell_pid)),
            Err(ReadError::Closed) => return Err(closed(self)),
        }
        // `J <nonce> <pid>` (no process: the input couldn't be passed on, `R` comes next).
        let mut line = match self.read_line(Some(deadline)) {
            Ok(line) => line,
            Err(ReadError::TimedOut) => return Err(Failure::TimedOut(shell_pid)),
            Err(ReadError::Closed) => return Err(closed(self)),
        };
        let pid = parse_job(&String::from_utf8_lossy(&line), nonce);
        if pid.is_some() {
            line = match self.read_line(Some(deadline)) {
                Ok(line) => line,
                Err(ReadError::TimedOut) => return Err(Failure::TimedOut(pid)),
                Err(ReadError::Closed) => return Err(closed(self)),
            };
        }
        let Some((code, out_len, err_len)) = parse_header(&String::from_utf8_lossy(&line), nonce)
        else {
            return Err(lost(self, "it answered out of turn"));
        };
        // The command ended; what it left running (and the shell) is stopped if its output
        // doesn't come in time.
        let timed_out_or_closed = |e: ReadError, s: &Self| match e {
            ReadError::TimedOut => Failure::TimedOut(shell_pid),
            ReadError::Closed => closed(s),
        };
        let stdout = self
            .read_exactly(out_len, Some(deadline))
            .map_err(|e| timed_out_or_closed(e, self))?;
        let stderr = self
            .read_exactly(err_len, Some(deadline))
            .map_err(|e| timed_out_or_closed(e, self))?;
        self.used = Instant::now();
        Ok(Output {
            code,
            stdout,
            stderr,
        })
    }

    /// More of the output, by `deadline` if there is one.
    fn more(&mut self, deadline: Option<Instant>) -> Result<(), ReadError> {
        let chunk = match deadline {
            None => self.chunks.recv().map_err(|_| ReadError::Closed)?,
            Some(at) => {
                let left = at.saturating_duration_since(Instant::now());
                self.chunks.recv_timeout(left).map_err(|e| match e {
                    RecvTimeoutError::Timeout => ReadError::TimedOut,
                    RecvTimeoutError::Disconnected => ReadError::Closed,
                })?
            }
        };
        self.pending.extend_from_slice(&chunk);
        Ok(())
    }

    /// The next line, its `\n` included.
    fn read_line(&mut self, deadline: Option<Instant>) -> Result<Vec<u8>, ReadError> {
        let mut searched = 0;
        loop {
            if let Some(i) = self.pending[searched..].iter().position(|&b| b == b'\n') {
                let rest = self.pending.split_off(searched + i + 1);
                return Ok(std::mem::replace(&mut self.pending, rest));
            }
            searched = self.pending.len();
            self.more(deadline)?;
        }
    }

    /// The next `len` bytes, held only as they arrive: a header that says more than comes
    /// doesn't make the app set aside memory for it.
    fn read_exactly(
        &mut self,
        len: usize,
        deadline: Option<Instant>,
    ) -> Result<Vec<u8>, ReadError> {
        while self.pending.len() < len {
            self.more(deadline)?;
        }
        let rest = self.pending.split_off(len);
        Ok(std::mem::replace(&mut self.pending, rest))
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

impl Drop for Shell {
    fn drop(&mut self) {
        // Its shell ends (and clears its folder) as the connection goes.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What to make of what ssh said when it couldn't connect.
fn connect_hint(said: &str) -> &'static str {
    if said.contains("Host key verification failed") {
        " - its host key wasn't trusted"
    } else if said.contains("Permission denied") {
        " - check the user, the key or the password"
    } else {
        ""
    }
}

/// Reads the process's output as it comes, for [`Shell::more`].
fn read_output(mut stdout: std::process::ChildStdout) -> Receiver<Vec<u8>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        loop {
            let mut buf = vec![0u8; 64 * 1024];
            match stdout.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.truncate(n);
                    if tx.send(buf).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
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

/// What a process that has ended said on `stderr`: read to its end, but waited for `within` at
/// most, as a proxy it started can keep the pipe open after it.
pub(super) fn said_on_ending(stderr: std::process::ChildStderr, within: Duration) -> String {
    let said = Arc::new(Mutex::new(String::new()));
    let drain = drain_errors(stderr, Arc::clone(&said));
    wait_for(&drain, within);
    lock(&said).trim().to_owned()
}

/// Waits for `drain` to end, `within` at most.
fn wait_for(drain: &std::thread::JoinHandle<()>, within: Duration) {
    let until = Instant::now() + within;
    while !drain.is_finished() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A lock, also when a thread panicked holding it: what it guards stays usable.
pub(super) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::remote::session::{Request, framed};

    fn local_shell() -> Result<Shell, String> {
        Shell::start(Command::new("sh"), "test")
    }

    fn run_in(shell: &mut Shell, req: &Request) -> Result<Output, String> {
        let nonce = nonce();
        shell
            .run(
                &framed(req, &nonce),
                &nonce,
                Instant::now() + Duration::from_secs(30),
            )
            .map_err(Failure::message)
    }

    /// The framing runs as it is in a real `sh`, binary output and input included.
    #[test]
    fn a_framed_request_runs_in_sh() -> Result<(), String> {
        let mut shell = local_shell()?;
        let req = Request {
            script: "cat; printf 'err' >&2; exit 3".to_owned(),
            stdin: Some(vec![0, 1, 2, b'\n', 255]),
            ..Request::default()
        };
        let out = run_in(&mut shell, &req)?;
        assert_eq!(out.code, 3);
        assert_eq!(out.stdout, vec![0, 1, 2, b'\n', 255]);
        assert_eq!(out.stderr, b"err");
        let out = run_in(
            &mut shell,
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
    #[test]
    fn output_written_late_stays_out_of_the_next_answer() -> Result<(), String> {
        let mut shell = local_shell()?;
        let late = Request {
            script: "printf a; (sleep 0.2; printf bbbbbbbb) &".to_owned(),
            ..Request::default()
        };
        assert!(run_in(&mut shell, &late)?.stdout.starts_with(b"a"));
        std::thread::sleep(Duration::from_millis(400));
        let next = Request {
            script: "printf next".to_owned(),
            ..Request::default()
        };
        assert_eq!(run_in(&mut shell, &next)?.stdout, b"next");
        Ok(())
    }

    /// A folder that isn't there says so in words the caller knows.
    #[test]
    fn a_missing_folder_is_told_apart() -> Result<(), String> {
        let mut shell = local_shell()?;
        let req = Request {
            cwd: Some("/no/such/folder".to_owned()),
            script: "true".to_owned(),
            ..Request::default()
        };
        let out = run_in(&mut shell, &req)?;
        assert_eq!(out.code, 1);
        assert!(crate::remote::session::is_missing_folder(&out.stderr));
        Ok(())
    }

    /// A shell that never says the command started is given up on by the deadline, and its own
    /// process (as it said when ready) is the one to stop.
    #[test]
    fn a_command_that_never_starts_times_out() -> Result<(), String> {
        let mut shell = local_shell()?;
        let pid = shell.remote_pid.ok_or("the shell didn't say its process")?;
        let nonce = nonce();
        let started = Instant::now();
        // Not framed: it never says `S`.
        let ran = shell.run(
            "sleep 5\n",
            &nonce,
            Instant::now() + Duration::from_millis(300),
        );
        assert!(matches!(ran, Err(Failure::TimedOut(Some(p))) if p == pid));
        assert!(started.elapsed() < Duration::from_secs(3));
        Ok(())
    }

    /// A shell idle a while is checked before it's used, and one that ended is not used.
    #[test]
    fn idle_shells_are_checked() -> Result<(), String> {
        let mut shell = local_shell()?;
        shell.used = Instant::now() - CHECK_AFTER - Duration::from_secs(1);
        assert!(shell.usable());
        let _ = shell.child.kill();
        let _ = shell.child.wait();
        assert!(!shell.usable());
        Ok(())
    }
}
