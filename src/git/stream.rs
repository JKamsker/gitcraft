//! A git command whose output is wanted as it comes, line by line (`git bisect run`, which can
//! take as long as the tests it runs), here or on the repository's machine, and which can be
//! stopped with everything it started.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::command::{Extra, git_cmd, remote_request, with_profile};
use crate::remote::{self, Host};

/// What the script on another machine prints first: the process of the shell it runs in,
/// which is stopped (with all it started) to stop the command.
const PID_MARK: &str = "gitcraft-stream-pid";

/// How long a command asked to stop has before it is killed.
const GRACE: Duration = Duration::from_secs(2);

/// Stops a streamed command, from any thread, before or while it runs.
#[derive(Clone, Default)]
pub struct Stopper(Arc<Mutex<Stopping>>);

#[derive(Default)]
struct Stopping {
    stopped: bool,
    ended: bool,
    process: Option<Process>,
}

/// What a running command is, to stop it.
#[derive(Clone)]
enum Process {
    /// Here: its process, which leads a process group of its own on Unix.
    Here(u32),
    /// On `host`: the shell there it runs in, and the `ssh` / `wsl.exe` process here.
    There { host: Host, pid: u32, local: u32 },
}

impl Stopper {
    /// Stops the command (or keeps it from starting), with what it started.
    pub fn stop(&self) {
        let process = match self.0.lock() {
            Ok(mut s) => {
                s.stopped = true;
                s.process.take()
            }
            Err(_) => None,
        };
        if let Some(process) = process {
            self.kill(process);
        }
    }

    /// Whether [`Self::stop`] was asked for.
    pub fn stopped(&self) -> bool {
        self.0.lock().map(|s| s.stopped).unwrap_or(true)
    }

    /// The command started as `process`: stopped right away if that was asked for already.
    fn started(&self, process: Process) {
        let stop_now = match self.0.lock() {
            Ok(mut s) if !s.stopped => {
                s.process = Some(process.clone());
                false
            }
            _ => true,
        };
        if stop_now {
            self.kill(process);
        }
    }

    fn ended(&self) {
        if let Ok(mut s) = self.0.lock() {
            s.ended = true;
            s.process = None;
        }
    }

    fn kill(&self, process: Process) {
        match process {
            Process::Here(pid) => kill_here(self.clone(), pid, true),
            Process::There { host, pid, local } => {
                remote::stop(host, pid);
                kill_here(self.clone(), local, false);
            }
        }
    }

    fn has_ended(&self) -> bool {
        self.0.lock().map(|s| s.ended).unwrap_or(true)
    }
}

/// Ends the process `pid` of this machine (and its process group, `group`), in the background:
/// asked politely first, then killed if it hasn't ended after [`GRACE`].
fn kill_here(stopper: Stopper, pid: u32, group: bool) {
    std::thread::spawn(move || {
        #[cfg(unix)]
        {
            let target = if group {
                format!("-{pid}")
            } else {
                pid.to_string()
            };
            let signal = |name: &str| {
                let _ = Command::new("kill")
                    .args([name, "--", &target])
                    .stderr(Stdio::null())
                    .status();
            };
            signal("-TERM");
            std::thread::sleep(GRACE);
            if !stopper.has_ended() {
                signal("-KILL");
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = (&stopper, group);
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
    });
}

/// How a streamed command ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ended {
    /// Its exit code (`None`: ended by a signal).
    pub code: Option<i32>,
    /// It was stopped with its [`Stopper`].
    pub stopped: bool,
}

/// Runs git with `args` in `dir` (on whichever machine it is), giving each line it prints, on
/// either output, to `line` as it comes, until it ends or `stopper` stops it. For a background
/// thread: it takes as long as the command.
pub fn stream(
    dir: &Path,
    args: &[&str],
    stopper: &Stopper,
    line: impl FnMut(String),
) -> Result<Ended, String> {
    if stopper.stopped() {
        return Ok(Ended {
            code: None,
            stopped: true,
        });
    }
    let result = match remote::parse(dir) {
        Some(target) => stream_there(&target, args, stopper, line),
        None => stream_here(dir, args, stopper, line),
    };
    stopper.ended();
    result.map(|code| Ended {
        code,
        stopped: stopper.stopped(),
    })
}

fn stream_here(
    dir: &Path,
    args: &[&str],
    stopper: &Stopper,
    line: impl FnMut(String),
) -> Result<Option<i32>, String> {
    let mut cmd = git_cmd();
    with_profile(&mut cmd, args);
    cmd.arg("-C").arg(dir).args(args);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own process group, so that stopping it stops what it runs too.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("could not run git: {e}"))?;
    stopper.started(Process::Here(child.id()));
    let (tx, rx) = channel();
    let readers = [
        child.stdout.take().map(|out| read_lines(out, tx.clone())),
        child.stderr.take().map(|err| read_lines(err, tx.clone())),
    ];
    drop(tx);
    rx.into_iter().for_each(line);
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let status = child
        .wait()
        .map_err(|e| format!("could not run git: {e}"))?;
    Ok(status.code())
}

fn stream_there(
    target: &remote::Target,
    args: &[&str],
    stopper: &Stopper,
    mut line: impl FnMut(String),
) -> Result<Option<i32>, String> {
    let mut req = remote_request(target, &Extra::default(), args);
    // `$$` in the subshell the request runs in is the shell's own: the one that runs it all.
    req.script = format!("printf '%s %s\\n' {PID_MARK} \"$$\"\n{} 2>&1", req.script);
    let mut child = remote::spawn(&target.host, &req)?;
    let local = child.id();
    let (tx, rx) = channel();
    let readers = [
        child.stdout.take().map(|out| read_lines(out, tx.clone())),
        // What ssh itself says (a dropped connection).
        child.stderr.take().map(|err| read_lines(err, tx.clone())),
    ];
    drop(tx);
    let mut started = false;
    for text in rx {
        if !started
            && let Some(pid) = text
                .strip_prefix(PID_MARK)
                .and_then(|p| p.trim().parse().ok())
        {
            started = true;
            stopper.started(Process::There {
                host: target.host.clone(),
                pid,
                local,
            });
            continue;
        }
        line(text);
    }
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let status = child
        .wait()
        .map_err(|e| format!("could not reach {}: {e}", target.host.label()))?;
    Ok(status.code())
}

/// Reads `from` line by line (as text, whatever its bytes) into `to`, on a thread of its own.
fn read_lines(from: impl Read + Send + 'static, to: Sender<String>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(from);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buf);
                    if to
                        .send(text.trim_end_matches(['\n', '\r']).to_owned())
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> Result<std::path::PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-stream-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        super::super::run(Some(&dir), &["init", "-q"])?;
        Ok(dir)
    }

    /// Both outputs come through, as they come, and the exit code with them.
    #[test]
    fn lines_of_both_outputs_come_through() -> Result<(), String> {
        let dir = repo("lines")?;
        let mut lines = Vec::new();
        let ended = stream(
            &dir,
            &["-c", "alias.say=!echo one; echo two >&2; exit 3", "say"],
            &Stopper::default(),
            |l| lines.push(l),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let ended = ended?;
        lines.sort();
        assert_eq!(lines, ["one", "two"]);
        assert_eq!(
            ended,
            Ended {
                code: Some(3),
                stopped: false
            }
        );
        Ok(())
    }

    /// A command stopped before it starts never runs.
    #[test]
    fn a_command_stopped_before_it_starts_never_runs() -> Result<(), String> {
        let dir = repo("early")?;
        let stopper = Stopper::default();
        stopper.stop();
        let mut lines = Vec::new();
        let ended = stream(&dir, &["-c", "alias.say=!echo ran", "say"], &stopper, |l| {
            lines.push(l)
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ended?.stopped);
        assert!(lines.is_empty());
        Ok(())
    }

    /// Stopping ends git and what it runs, however deep.
    #[cfg(unix)]
    #[test]
    fn stopping_ends_the_command_and_what_it_started() -> Result<(), String> {
        let dir = repo("stop")?;
        let marker = format!("{}.25", std::process::id() % 1000 + 300);
        let stopper = Stopper::default();
        let stopping = stopper.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            stopping.stop();
        });
        let started = std::time::Instant::now();
        let alias = format!("alias.wait=!sh -c 'sleep {marker}; true'; true");
        let ended = stream(&dir, &["-c", &alias, "wait"], &stopper, |_| {});
        let took = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ended?.stopped);
        assert!(took < Duration::from_secs(30), "{took:?}");
        let left = Command::new("sh")
            .args([
                "-c",
                &format!("ps -A -o args | grep -c '^sleep {marker}$' || true"),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(String::from_utf8_lossy(&left.stdout).trim(), "0");
        Ok(())
    }
}
