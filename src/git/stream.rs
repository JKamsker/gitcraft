//! A git command whose output is wanted as it comes, line by line (`git bisect run`, which can
//! take as long as the tests it runs), here or on the repository's machine, and which can be
//! stopped with everything it started.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::command::{Extra, git_cmd, remote_request, with_profile};
use crate::remote::{self, Host};

/// What the script on another machine prints first: the process of the shell it runs in,
/// which is stopped (with all it started) to stop the command.
const PID_MARK: &str = "gitcraft-stream-pid";

/// What it prints last, with the command's exit code.
const END_MARK: &str = "gitcraft-stream-end";

/// How often a command's end is looked for while it prints nothing.
const POLL: Duration = Duration::from_millis(100);

/// How long what a command printed last is waited for once it ended.
const DRAIN: Duration = Duration::from_millis(300);

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
            // That machine's processes first: with `ssh` gone, what the command started could
            // lose its parent and be missed.
            Process::There { host, pid, local } => {
                let stopper = self.clone();
                std::thread::spawn(move || {
                    let _ = remote::stop(host, pid).join();
                    kill_here(stopper, local, false);
                });
            }
        }
    }

    fn has_ended(&self) -> bool {
        self.0.lock().map(|s| s.ended).unwrap_or(true)
    }
}

/// Ends the process `pid` of this machine (and its process group, `group`): asked politely
/// right away (so that it is asked even when the app is quitting), then killed in the
/// background if it hasn't ended after [`GRACE`].
fn kill_here(stopper: Stopper, pid: u32, group: bool) {
    #[cfg(unix)]
    {
        let target = if group {
            format!("-{pid}")
        } else {
            pid.to_string()
        };
        let signal = move |name: &str| {
            let _ = Command::new("kill")
                .args([name, "--", &target])
                .stderr(Stdio::null())
                .status();
        };
        signal("-TERM");
        std::thread::spawn(move || {
            std::thread::sleep(GRACE);
            // A group is killed whether git ended or not: what it ran may have outlived it,
            // ignoring the polite ask.
            if group || !stopper.has_ended() {
                signal("-KILL");
            }
        });
    }
    // No polite way to ask there: the whole tree is ended at once.
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
    let _timer = super::backend::CliTimer::new(args);
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
    let mut line = line;
    // Until git has ended: something it left running in the background (a server a test
    // started) may hold its outputs open for good.
    let status = loop {
        match rx.recv_timeout(POLL) {
            Ok(text) => line(text),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                break child
                    .wait()
                    .map_err(|e| format!("could not run git: {e}"))?;
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("could not run git: {e}"))?
        {
            break status;
        }
    };
    // What it printed last, still on its way.
    drain(&rx, &mut line);
    for reader in readers.into_iter().flatten().filter(|r| r.is_finished()) {
        let _ = reader.join();
    }
    Ok(status.code())
}

/// Takes the lines still coming for a moment after the command ended.
fn drain(rx: &Receiver<String>, line: &mut impl FnMut(String)) {
    let until = Instant::now() + DRAIN;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(text) => line(text),
            Err(_) => return,
        }
    }
}

fn stream_there(
    target: &remote::Target,
    args: &[&str],
    stopper: &Stopper,
    mut line: impl FnMut(String),
) -> Result<Option<i32>, String> {
    let mut req = remote_request(target, &Extra::default(), args);
    req.script = there_script(&req.script);
    let mut child = remote::spawn(&target.host, &req)?;
    let local = child.id();
    let (tx, rx) = channel();
    let readers = [
        child.stdout.take().map(|out| read_lines(out, tx.clone())),
        // What ssh itself says (a dropped connection).
        child.stderr.take().map(|err| read_lines(err, tx.clone())),
    ];
    drop(tx);
    let said_end = read_there(&rx, &mut line, |pid| {
        stopper.started(Process::There {
            host: target.host.clone(),
            pid,
            local,
        });
    });
    if let Some(code) = said_end {
        // Ended there: the connection goes, whatever still holds it open.
        let _ = child.kill();
        let _ = child.wait();
        return Ok(Some(code));
    }
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let status = child
        .wait()
        .map_err(|e| format!("could not reach {}: {e}", target.host.label()))?;
    Ok(status.code())
}

/// `script` (`exec git …`) as it runs on another machine: its shell's process said first (`$$`
/// in the subshell a request runs in is the shell's own, the one that runs it all), both
/// outputs as one, and its end said in a line of its own with the exit code, since something
/// it left running in the background may hold the connection open for good.
fn there_script(script: &str) -> String {
    let command = script.strip_prefix("exec ").unwrap_or(script);
    format!(
        "printf '%s %s\\n' {PID_MARK} \"$$\"\n{command} 2>&1\nprintf '\\n%s %s\\n' {END_MARK} \"$?\""
    )
}

/// Reads what [`there_script`] prints from `rx`, giving its process to `started` and the
/// command's lines to `line`, up to its end: the exit code it says, `None` when the output
/// ended first (the connection dropped).
fn read_there(
    rx: &Receiver<String>,
    line: &mut impl FnMut(String),
    mut started: impl FnMut(u32),
) -> Option<i32> {
    let mut known = false;
    // An empty line is held back: the one before the end's is the script's own.
    let mut blank = false;
    for text in rx {
        if let Some(code) = text.strip_prefix(END_MARK) {
            return code.trim().parse().ok();
        }
        if !known
            && let Some(pid) = text
                .strip_prefix(PID_MARK)
                .and_then(|p| p.trim().parse().ok())
        {
            known = true;
            started(pid);
            continue;
        }
        if std::mem::replace(&mut blank, text.is_empty()) {
            line(String::new());
        }
        if !blank {
            line(text);
        }
    }
    None
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

    /// What runs on another machine says its process, its lines (blank ones too) and its end,
    /// even when something it left in the background holds the output open. Run here by `sh`,
    /// as it is there.
    #[cfg(unix)]
    #[test]
    fn a_command_elsewhere_says_its_end_whatever_it_leaves_running() -> Result<(), String> {
        let script = there_script("exec sh -c 'echo one; echo; echo two >&2; sleep 33 & exit 3'");
        let mut child = Command::new("sh")
            .args(["-c", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let (tx, rx) = channel();
        let _reader = child.stdout.take().map(|out| read_lines(out, tx));
        let mut lines = Vec::new();
        let mut pid = None;
        let started = Instant::now();
        let code = read_there(&rx, &mut |l| lines.push(l), |p| pid = Some(p));
        let _ = child.wait();
        assert!(started.elapsed() < Duration::from_secs(20));
        assert_eq!(code, Some(3));
        assert_eq!(pid, Some(child.id()));
        assert_eq!(lines, ["one", "", "two"]);
        Ok(())
    }

    /// A command that leaves something running with its outputs is over when git is.
    #[cfg(unix)]
    #[test]
    fn a_command_is_over_when_git_is() -> Result<(), String> {
        let dir = repo("background")?;
        let started = Instant::now();
        let mut lines = Vec::new();
        let ended = stream(
            &dir,
            &["-c", "alias.bg=!sleep 34 & echo started", "bg"],
            &Stopper::default(),
            |l| lines.push(l),
        );
        let took = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ended?.code, Some(0));
        assert_eq!(lines, ["started"]);
        assert!(took < Duration::from_secs(20), "{took:?}");
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
