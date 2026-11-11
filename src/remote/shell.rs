//! One shell kept open on another machine, its output read by a thread of its own, so that
//! the few reads that must not wait for good (the connection being made, the check of a shell
//! left idle) can have a deadline.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::session::{Output, nonce, parse_header, parse_ready, setup_script};

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
    TimedOut(bool),
}

impl Failure {
    pub(super) fn message(self) -> String {
        match self {
            Failure::NotSent(m) | Failure::Lost(m) => m,
            Failure::TimedOut(true) => {
                "it took too long; the owned supervisor confirmed stop and reap".to_owned()
            }
            Failure::TimedOut(false) => {
                "it took too long; cleanup is unknown and recovery evidence was retained".to_owned()
            }
        }
    }
}

enum ReadError {
    Closed,
    TimedOut,
}

pub(super) struct Shell {
    child: gitcraft_os::OwnedChild,
    stdin: Option<gitcraft_os::BoundedInput>,
    /// What the shell printed, as the reader thread got it.
    chunks: Receiver<Vec<u8>>,
    /// Read and not taken yet.
    pending: Vec<u8>,
    /// The end of what the process said on its error output (ssh's own errors).
    said: Arc<Mutex<String>>,
    label: String,
    /// When it last answered.
    pub(super) used: Instant,
    /// Authenticated owner of the current request. No PID fallback is retained.
    lease: Option<super::supervisor::Endpoint>,
    cleaning: bool,
}

impl Shell {
    /// Observe this owned SSH client's exit without probing or sending a command.
    #[cfg(all(test, unix))]
    pub(super) fn exited(&mut self) -> std::io::Result<bool> {
        self.child.observe().map(|status| status.is_some())
    }

    /// Starts `cmd` (a shell on the machine `label` names), waits for it to be ready.
    pub(super) fn start(mut cmd: Command, label: &str) -> Result<Self, String> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = gitcraft_os::OwnedChild::spawn(cmd)
            .map_err(|e| format!("could not reach {label}: {e}"))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin(), child.stdout(), child.stderr())
        else {
            let _ = child.finish();
            return Err(format!("could not reach {label}"));
        };
        let stdin = gitcraft_os::BoundedInput::new(stdin)
            .map_err(|e| format!("could not prepare {label} input: {e}"))?;
        let said = Arc::new(Mutex::new(String::new()));
        let drain = drain_errors(stderr, Arc::clone(&said));
        let mut shell = Self {
            child,
            stdin: Some(stdin),
            chunks: read_output(stdout),
            pending: Vec::new(),
            said,
            label: label.to_owned(),
            used: Instant::now(),
            lease: None,
            cleaning: false,
        };
        match shell.handshake() {
            Ok(()) => {
                // What ssh said while connecting (a host key added…) says nothing about a
                // connection lost later.
                lock(&shell.said).clear();
                Ok(shell)
            }
            Err(e) => {
                shell.stdin.take();
                let _ = shell.child.finish();
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
        let mut deadline = Instant::now() + CONNECT_FOR;
        let cancel = crate::tasks::cancellation();
        self.stdin
            .as_mut()
            .ok_or("remote controller closed")?
            .write_until(setup_script().into_bytes(), || {
                if cancel.is_cancelled() {
                    return false;
                }
                if Instant::now() < deadline {
                    return true;
                }
                if super::askpass::asked_lately(CONNECT_FOR) {
                    deadline = Instant::now() + CONNECT_FOR;
                    true
                } else {
                    false
                }
            })
            .map_err(|e| lost(&e))?;
        loop {
            match self.read_line(Some(deadline)) {
                Ok(line) => {
                    if parse_ready(&line) {
                        return Ok(());
                    }
                }
                Err(ReadError::Closed) => return Err(lost(&"the connection closed")),
                // The user is being asked something (a password): their time, not the network's.
                Err(ReadError::TimedOut)
                    if !crate::tasks::cancellation().is_cancelled()
                        && super::askpass::asked_lately(CONNECT_FOR) =>
                {
                    deadline = Instant::now() + CONNECT_FOR;
                }
                Err(ReadError::TimedOut) => return Err(lost(&"it didn't answer in time")),
            }
        }
    }

    /// Whether it can take a command: its process is running and, after a while idle, it
    /// answers a check in time.
    pub(super) fn usable(&mut self) -> bool {
        if self.child.observe().map_or(true, |status| status.is_some()) {
            return false;
        }
        if self.used.elapsed() < CHECK_AFTER {
            return true;
        }
        let nonce = nonce();
        let check = format!("P {nonce}\n");
        let deadline = Instant::now() + CHECK_FOR;
        let cancel = crate::tasks::cancellation();
        if self.stdin.as_mut().is_none_or(|stdin| {
            stdin
                .write_until(check.into_bytes(), || {
                    Instant::now() < deadline && !cancel.is_cancelled()
                })
                .is_err()
        }) {
            return false;
        }
        let answered = self
            .read_line(Some(deadline))
            .is_ok_and(|line| line.trim_ascii_end() == format!("P {nonce}").as_bytes());
        if answered {
            self.used = Instant::now();
        }
        answered
    }

    /// Dispatch once, correlate exact frames, and retain the owner through bounded EOF
    /// cleanup. A timeout returns acknowledged stopped versus unknown distinctly.
    pub(super) fn run(
        &mut self,
        script: &str,
        nonce: &str,
        deadline: Instant,
    ) -> Result<Output, Failure> {
        lock(&self.said).clear();
        self.lease = None;
        self.cleaning = false;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Failure::NotSent("remote controller closed".into()))?;
        // Every write failure is unknown, including partial delivery or flush failure.
        let cancel = crate::tasks::cancellation();
        stdin
            .write_until(script.as_bytes().to_vec(), || {
                Instant::now() < deadline && !cancel.is_cancelled()
            })
            .map_err(|e| {
                Failure::Lost(format!(
                    "{}; command delivery/cleanup unknown",
                    self.lost(&e.to_string())
                ))
            })?;
        let mut deadline = deadline;
        let mut expired = false;
        let start = self.protocol_line(&mut deadline, &mut expired)?;
        if start.trim_ascii_end() != format!("S {nonce}").as_bytes() {
            return Err(Failure::Lost(
                self.lost("it answered out of turn; cleanup unknown"),
            ));
        }
        let line = self.protocol_line(&mut deadline, &mut expired)?;
        self.lease = super::supervisor::Endpoint::parse(&line, nonce);
        let Some(owner) = self.lease.clone() else {
            return Err(Failure::Lost(
                self.lost("invalid ownership handshake; cleanup unknown"),
            ));
        };
        let receipt = self.protocol_line(&mut deadline, &mut expired)?;
        if !receipt
            .strip_prefix(b"C1 ")
            .is_some_and(|bytes| owner.confirms(bytes))
        {
            return Err(Failure::Lost(
                self.lost("missing terminal receipt; cleanup unknown"),
            ));
        }
        let header = self.protocol_line(&mut deadline, &mut expired)?;
        let Some((code, out_len, err_len)) = parse_header(&String::from_utf8_lossy(&header), nonce)
        else {
            return Err(Failure::Lost(self.lost("invalid output header")));
        };
        let stdout = self.protocol_bytes(out_len, &mut deadline, &mut expired)?;
        let stderr = self.protocol_bytes(err_len, &mut deadline, &mut expired)?;
        self.used = Instant::now();
        if !receipt
            .strip_prefix(b"C1 ")
            .is_some_and(|bytes| owner.complete(bytes))
        {
            return Err(Failure::Lost(
                "owned command stopped but output delivery is incomplete".into(),
            ));
        }
        if expired {
            return Err(Failure::TimedOut(true));
        }
        // Only the complete receipt and frozen binary output permit retiring evidence.
        // This uses the same connection, including passive reads; no reconnect is needed.
        let ack = format!("A {nonce}\n");
        let ack_deadline = deadline.min(Instant::now() + CHECK_FOR);
        if self.stdin.as_mut().is_none_or(|stdin| {
            stdin
                .write_until(ack.into_bytes(), || {
                    Instant::now() < ack_deadline && !cancel.is_cancelled()
                })
                .is_err()
        }) {
            self.stdin.take();
        }
        self.lease = None;
        Ok(Output {
            code,
            stdout,
            stderr,
        })
    }

    fn begin_cleanup(&mut self, deadline: &mut Instant, expired: &mut bool) -> Result<(), Failure> {
        if *expired {
            return Err(Failure::TimedOut(false));
        }
        *expired = true;
        self.cleaning = true;
        // Controller EOF asks the same owner to stop, reap, persist and acknowledge. This
        // uses no second connection, including passive reads and pre-handshake cancellation.
        self.stdin.take();
        *deadline = Instant::now() + Duration::from_secs(5);
        Ok(())
    }
    fn protocol_line(
        &mut self,
        deadline: &mut Instant,
        expired: &mut bool,
    ) -> Result<Vec<u8>, Failure> {
        loop {
            match self.read_line(Some(*deadline)) {
                Ok(line) => return Ok(line),
                Err(ReadError::TimedOut) => self.begin_cleanup(deadline, expired)?,
                Err(ReadError::Closed) => {
                    return Err(Failure::Lost(self.lost("controller lost; cleanup unknown")));
                }
            }
        }
    }
    fn protocol_bytes(
        &mut self,
        len: usize,
        deadline: &mut Instant,
        expired: &mut bool,
    ) -> Result<Vec<u8>, Failure> {
        loop {
            match self.read_exactly(len, Some(*deadline)) {
                Ok(bytes) => return Ok(bytes),
                Err(ReadError::TimedOut) => self.begin_cleanup(deadline, expired)?,
                Err(ReadError::Closed) => {
                    return Err(Failure::Lost(self.lost("output delivery lost")));
                }
            }
        }
    }

    /// More of the output, by `deadline` if there is one.
    fn more(&mut self, deadline: Option<Instant>) -> Result<(), ReadError> {
        let cancel = crate::tasks::cancellation();
        let chunk = loop {
            if deadline.is_some_and(|at| Instant::now() >= at)
                || (!self.cleaning && cancel.is_cancelled())
            {
                return Err(ReadError::TimedOut);
            }
            let left = deadline.map_or(Duration::from_millis(50), |at| {
                at.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(50))
            });
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => break chunk,
                Err(RecvTimeoutError::Disconnected) => return Err(ReadError::Closed),
                Err(RecvTimeoutError::Timeout)
                    if deadline.is_some_and(|at| Instant::now() >= at) =>
                {
                    return Err(ReadError::TimedOut);
                }
                Err(RecvTimeoutError::Timeout) => {}
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
            if self.pending.len() > 65536 {
                return Err(ReadError::Closed);
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
        self.stdin.take();
        let _ = self.child.finish();
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
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
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
mod tests;
