//! Automatic forecasts must stop waiting when their editor disappears or their deadline
//! expires, including when a descendant holds an output pipe open after its parent exits.

use std::io::{Read, Write};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

pub(crate) fn run(
    command: Command,
    active: &dyn Fn() -> bool,
    limit: Duration,
) -> Result<Output, String> {
    run_with_input(command, None, active, limit)
}

/// The same deadline, cancellation and output caps, also while passing bounded input.
pub(crate) fn run_with_input(
    command: Command,
    input: Option<Vec<u8>>,
    active: &dyn Fn() -> bool,
    limit: Duration,
) -> Result<Output, String> {
    run_input(command, input, active, limit, MAX_OUTPUT, MAX_OUTPUT)
}

/// Shared process contract: independent stream caps, with the same input/deadline lifecycle.
pub(crate) fn run_input(
    mut command: Command,
    input: Option<Vec<u8>>,
    active: &dyn Fn() -> bool,
    limit: Duration,
    max_stdout: u64,
    max_stderr: u64,
) -> Result<Output, String> {
    if !active() {
        return Err("Forecast cancelled".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = Running(
        command
            .spawn()
            .map_err(|e| format!("Could not start forecast command: {e}"))?,
        false,
    );
    let input = input.map(|bytes| {
        let stdin = child.0.stdin.take();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = stdin
                .ok_or("No command stdin".to_owned())
                .and_then(|mut stdin| stdin.write_all(&bytes).map_err(|e| e.to_string()))
                .map(|_| Vec::new());
            let _ = tx.send(result);
        });
        rx
    });
    let mut written = input.is_none();
    let stdout = drain(
        child.0.stdout.take().ok_or("No forecast stdout")?,
        max_stdout,
    );
    let stderr = drain(
        child.0.stderr.take().ok_or("No forecast stderr")?,
        max_stderr,
    );
    let deadline = Instant::now() + limit;
    let (mut out, mut err, mut status) = (None, None, None);
    loop {
        if !active() {
            return Err("Forecast cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("Forecast command timed out".into());
        }
        if !written && let Some(input) = &input {
            written = receive(input)?.is_some();
        }
        if out.is_none() {
            out = receive(&stdout)?;
        }
        if err.is_none() {
            err = receive(&stderr)?;
        }
        if status.is_none() {
            status = child.0.try_wait().map_err(|e| e.to_string())?;
        }
        if written
            && status.is_some()
            && out.is_some()
            && err.is_some()
            && let (Some(status), Some(stdout), Some(stderr)) = (status, out.take(), err.take())
        {
            child.1 = true;
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }
        std::thread::sleep(POLL);
    }
}

fn drain(
    mut pipe: impl Read + Send + 'static,
    max_output: u64,
) -> mpsc::Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = (&mut pipe)
            .take(max_output.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                if bytes.len() as u64 > max_output {
                    Err("Forecast output exceeded its limit".into())
                } else {
                    Ok(bytes)
                }
            });
        let _ = tx.send(result);
    });
    rx
}

fn receive(rx: &mpsc::Receiver<Result<Vec<u8>, String>>) -> Result<Option<Vec<u8>>, String> {
    match rx.try_recv() {
        Ok(bytes) => bytes.map(Some),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => Err("Forecast output reader stopped".into()),
    }
}

struct Running(Child, bool);

impl Drop for Running {
    fn drop(&mut self) {
        if self.1 {
            return;
        }
        #[cfg(unix)]
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Best effort for descendants too; do not wait on this cleanup utility.
            if let Ok(mut killer) = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &self.0.id().to_string()])
                .creation_flags(0x0800_0000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                std::thread::spawn(move || {
                    let deadline = Instant::now() + Duration::from_secs(1);
                    while matches!(killer.try_wait(), Ok(None)) && Instant::now() < deadline {
                        std::thread::sleep(POLL);
                    }
                    let _ = killer.kill();
                    let _ = killer.try_wait();
                });
            }
        }
        let _ = self.0.kill();
        // Bounded reap: cleanup itself must not keep a cancelled forecast alive.
        let deadline = Instant::now() + Duration::from_millis(200);
        while matches!(self.0.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn configured_stream_caps_are_independent_and_keep_boundary_bytes() {
        for (stdout, stderr, succeeds) in [(3, 2, true), (2, 2, false), (3, 1, false)] {
            let mut command = Command::new("sh");
            command.args(["-c", "printf abc; printf xy >&2"]);
            let result = run_input(
                command,
                None,
                &|| true,
                Duration::from_secs(2),
                stdout,
                stderr,
            );
            if succeeds {
                assert!(result.is_ok_and(|o| o.stdout == b"abc" && o.stderr == b"xy"));
            } else {
                assert!(result.is_err_and(|e| e.contains("output exceeded")));
            }
        }
    }

    #[test]
    fn bounded_input_is_delivered_and_a_blocked_writer_is_cancelled() {
        let mut cat = Command::new("cat");
        let result = run_with_input(
            cat,
            Some(b"history tips\n".to_vec()),
            &|| true,
            Duration::from_secs(2),
        );
        assert!(result.is_ok_and(|o| o.status.success() && o.stdout == b"history tips\n"));
        cat = Command::new("sh");
        cat.args(["-c", "sleep 30"]);
        let start = Instant::now();
        let result = run_with_input(
            cat,
            Some(vec![b'x'; 1024 * 1024]),
            &|| start.elapsed() < Duration::from_millis(100),
            Duration::from_secs(30),
        );
        assert!(result.is_err_and(|e| e.contains("cancelled")));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn stalled_commands_and_descendant_pipes_obey_deadlines_and_cancellation() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & wait"]);
        let start = Instant::now();
        assert!(
            run(command, &|| true, Duration::from_millis(150))
                .is_err_and(|e| e.contains("timed out"))
        );
        assert!(start.elapsed() < Duration::from_secs(2));

        let active = Arc::new(AtomicBool::new(true));
        let cancel = Arc::clone(&active);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            cancel.store(false, Ordering::Relaxed);
        });
        let mut command = Command::new("sh");
        // The shell exits; the child still owns stdout. Waiting only on the leader is wrong.
        command.args(["-c", "sleep 30 &"]);
        let start = Instant::now();
        assert!(
            run(
                command,
                &|| active.load(Ordering::Relaxed),
                Duration::from_secs(30)
            )
            .is_err_and(|e| e.contains("cancelled"))
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn cancellation_stops_descendants_instead_of_only_discarding_their_output() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let marker = std::env::temp_dir().join(format!(
            "gitcraft-cancelled-descendant-{}-{stamp}",
            std::process::id()
        ));
        let script = format!(
            "sleep 0.5; touch {}",
            crate::remote::sh_quote(&marker.to_string_lossy())
        );
        let mut command = Command::new("sh");
        command.args([
            "-c",
            &format!("sh -c {} & wait", crate::remote::sh_quote(&script)),
        ]);
        assert!(run(command, &|| true, Duration::from_millis(100)).is_err());
        std::thread::sleep(Duration::from_millis(600));
        assert!(!marker.exists());
    }
}
