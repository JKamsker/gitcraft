//! Automatic forecasts must stop waiting when their editor disappears or their deadline
//! expires, including when a descendant holds an output pipe open after its parent exits.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

pub(crate) fn run(
    mut command: Command,
    active: &dyn Fn() -> bool,
    limit: Duration,
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = Running(
        command
            .spawn()
            .map_err(|e| format!("Could not start forecast command: {e}"))?,
        false,
    );
    let stdout = drain(child.0.stdout.take().ok_or("No forecast stdout")?);
    let stderr = drain(child.0.stderr.take().ok_or("No forecast stderr")?);
    let deadline = Instant::now() + limit;
    let (mut out, mut err, mut status) = (None, None, None);
    loop {
        if !active() {
            return Err("Forecast cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("Forecast command timed out".into());
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
        if status.is_some()
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

fn drain(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = (&mut pipe)
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                if bytes.len() as u64 > MAX_OUTPUT {
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
