//! Running a git command that goes over the network (fetch, pull, push, clone) so that it can
//! be followed and stopped: its error output is read as it comes (with `--progress`, git says
//! "Receiving objects: 45%" there), and a cancel stops it, with everything it started, the way
//! a command that hung on another machine is stopped (see [`remote::stop_script`]).
//!
//! On another machine it runs in a process of its own ([`remote::spawn`]) rather than in a
//! kept-open shell, whose answer only comes once the command has ended; that process says the
//! command's process number there first, for stopping it.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use super::{
    Credentials, Extra, Recorded, after_options, extra_for, failure_message, local_command,
};
use crate::git::progress::{Progress, ProgressReader, without_progress};
use crate::remote::{self, Host, Target};

/// What a cancelled command fails with.
pub const CANCELLED: &str = "cancelled";

/// How the UI follows a running operation and stops it: shared with the thread that runs it.
pub struct OpControl {
    cancel: AtomicBool,
    progress: Mutex<Option<Progress>>,
    /// What each git call of the operation did so far, for the activity log.
    steps: Mutex<Vec<Recorded>>,
    /// Called whenever there is news (a repaint of the window).
    notify: Box<dyn Fn() + Send + Sync>,
}

impl OpControl {
    /// A control that calls `notify` when the progress moves.
    pub fn new(notify: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            cancel: AtomicBool::new(false),
            progress: Mutex::new(None),
            steps: Mutex::new(Vec::new()),
            notify: Box::new(notify),
        })
    }

    /// Asks the operation to stop: the command running is stopped, and no step after it runs.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        (self.notify)();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// The latest progress the running command reported.
    pub fn progress(&self) -> Option<Progress> {
        self.progress.lock().ok().and_then(|p| p.clone())
    }

    /// Keeps what a git call of the operation did.
    pub fn record(&self, step: Recorded) {
        if let Ok(mut steps) = self.steps.lock() {
            steps.push(step);
        }
        (self.notify)();
    }

    /// What the operation's git calls did so far.
    pub fn steps(&self) -> Vec<Recorded> {
        self.steps.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn set_progress(&self, progress: Option<Progress>) {
        if let Ok(mut p) = self.progress.lock() {
            *p = progress;
        }
        (self.notify)();
    }
}

/// The subcommands that take `--progress`.
const SHOWS_PROGRESS: [&str; 4] = ["fetch", "pull", "push", "clone"];

/// `args` with `--progress` right after the subcommand, for one that takes it and wasn't told
/// to be quiet (git leaves progress out on a pipe otherwise).
fn with_progress(args: &[&str]) -> Vec<String> {
    let rest = after_options(args);
    let at = args.len() - rest.len();
    let mut out: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let quiet = rest
        .iter()
        .any(|a| matches!(*a, "-q" | "--quiet" | "--progress"));
    if rest.first().is_some_and(|sub| SHOWS_PROGRESS.contains(sub)) && !quiet {
        out.insert(at + 1, "--progress".to_owned());
    }
    out
}

/// What the process run on another machine starts its error output with: then the process
/// number of git there.
const PID_MARKER: &str = "__GITCRAFT_GIT_PID__";

/// The request that runs git with `args` in `target`'s folder, saying its process number
/// first: a shell that prints its own and then becomes git (`exec`), so that the marker comes
/// before anything git says, and what came before it (a server's login banner) is told apart.
fn watched_request(target: &Target, extra: &Extra, args: &[&str]) -> remote::Request {
    let mut req = super::remote_request(target, extra, args);
    let git = req.script.strip_prefix("exec ").unwrap_or(&req.script);
    req.script = format!(
        "exec sh -c 'printf \"\\n%s %s\\n\" {PID_MARKER} \"$$\" >&2; exec \"$@\"' sh {git}"
    );
    req
}

/// How a followed command ended.
struct Ended {
    code: Option<i32>,
    status: String,
    stdout: String,
    stderr: String,
}

/// How often the command is looked at (cancelled? ended?).
const POLL: Duration = Duration::from_millis(40);

/// How long the output of a command that ended is waited for.
const DRAIN_FOR: Duration = Duration::from_secs(10);

/// How long a cancelled command on another machine is given to say its process number.
const PID_WAIT: Duration = Duration::from_secs(5);

/// Runs the git command `args` in `dir` (on whichever machine it is), as [`super::run_with`]
/// does (or [`super::run_unattended`] when `quiet`), following it through `control`: its
/// progress is reported there, and a cancel stops it (it then fails with [`CANCELLED`]).
pub fn run_watched(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
    control: &Arc<OpControl>,
) -> Result<String, String> {
    run_watched_recorded(dir, args, creds, quiet, control).0
}

/// [`run_watched`], and what the call did, for the activity log (with `--progress` in its
/// command line when it was added).
pub fn run_watched_recorded(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
    control: &Arc<OpControl>,
) -> (Result<String, String>, Recorded) {
    let started = Instant::now();
    if control.is_cancelled() {
        let not_run = Recorded::new(args, None, "Not run: cancelled".to_owned(), started);
        return (Err(CANCELLED.to_owned()), not_run);
    }
    let words = if quiet {
        args.iter().map(|a| (*a).to_owned()).collect()
    } else {
        with_progress(args)
    };
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let failed = |e: String| {
        let rec = Recorded::new(&words, None, e.clone(), started);
        (Err(e), rec)
    };
    let extra = extra_for(dir, creds, quiet);
    let target = dir.and_then(remote::parse);
    let spawned = match &target {
        Some(t) => remote::spawn(&t.host, &watched_request(t, &extra, &words))
            .map(|c| (c, Some(t.host.clone()))),
        None => spawn_here(dir, &extra, &words).map(|c| (c, None)),
    };
    let (child, host) = match spawned {
        Ok(spawned) => spawned,
        Err(e) => return failed(e),
    };
    let ended = match follow(child, host.as_ref(), control) {
        Ok(ended) => ended,
        Err(e) if e == CANCELLED => {
            return (
                Err(e),
                Recorded::new(&words, None, "Stopped: cancelled".to_owned(), started),
            );
        }
        Err(e) => return failed(e),
    };
    let recorded = Recorded::new(
        &words,
        ended.code,
        super::both(&ended.stdout, &ended.stderr),
        started,
    );
    if ended.code == Some(0) {
        return (Ok(ended.stdout), recorded);
    }
    if let Some(t) = &target
        && ended.code == Some(1)
        // After whatever the connection printed first (a login banner).
        && ended
            .stderr
            .lines()
            .any(|l| remote::is_missing_folder(l.as_bytes()))
    {
        let e = format!("{} isn't a folder on {}", t.path, t.host.label());
        return (Err(e), recorded);
    }
    let e = failure_message(
        args,
        &ended.status,
        &ended.stdout,
        &without_progress(&ended.stderr),
    );
    (Err(e), recorded)
}

fn spawn_here(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Result<Child, String> {
    local_command(dir, extra, args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run git: {e}"))
}

/// Waits for `child` (git here, or the process that runs it on `host`), reading its output and
/// reporting its progress, and stops it when `control` is cancelled (or, elsewhere, when it
/// runs past the time a network operation may take there).
fn follow(
    mut child: Child,
    host: Option<&Host>,
    control: &Arc<OpControl>,
) -> Result<Ended, String> {
    let (out_tx, out_rx) = mpsc::channel();
    if let Some(mut stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            let _ = out_tx.send(String::from_utf8_lossy(&bytes).into_owned());
        });
    }
    let remote_pid = Arc::new(AtomicU32::new(0));
    let (err_tx, err_rx) = mpsc::channel();
    if let Some(stderr) = child.stderr.take() {
        let reader = if host.is_some() {
            ProgressReader::with_marker(PID_MARKER)
        } else {
            ProgressReader::default()
        };
        read_errors(
            stderr,
            reader,
            Arc::clone(control),
            Arc::clone(&remote_pid),
            err_tx,
        );
    }
    let deadline = host.map(|_| Instant::now() + remote::NETWORK_LIMIT);
    let status = loop {
        // Ended first: what it did stands, even when Cancel came meanwhile.
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(format!("could not follow git: {e}")),
        }
        if control.is_cancelled() {
            stop(&mut child, host, &remote_pid);
            return Err(CANCELLED.to_owned());
        }
        if deadline.is_some_and(|d| Instant::now() > d) {
            stop(&mut child, host, &remote_pid);
            let minutes = remote::NETWORK_LIMIT.as_secs().div_ceil(60);
            let place = host.map(Host::label).unwrap_or_default();
            return Err(format!(
                "git didn't finish within {minutes} minutes on {place} and was stopped"
            ));
        }
        std::thread::sleep(POLL);
    };
    let code = status.code();
    Ok(Ended {
        code,
        status: match host {
            Some(_) => format!("exit status: {}", code.unwrap_or(-1)),
            None => status.to_string(),
        },
        // Something git left running (a hook's background job) may hold the pipes open.
        stdout: out_rx.recv_timeout(DRAIN_FOR).unwrap_or_default(),
        stderr: err_rx.recv_timeout(DRAIN_FOR).unwrap_or_else(|_| {
            "(git ended, but something it started kept its output open)".to_owned()
        }),
    })
}

/// Reads `stderr` in the background into `reader`, telling `control` each new progress line,
/// and `pid` the process number the marker line gives; sends the text read when it ends.
fn read_errors(
    mut stderr: std::process::ChildStderr,
    mut reader: ProgressReader,
    control: Arc<OpControl>,
    pid: Arc<AtomicU32>,
    done: mpsc::Sender<String>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if reader.feed(&buf[..n]) {
                        control.set_progress(reader.latest().cloned());
                    }
                    if pid.load(Ordering::SeqCst) == 0
                        && let Some(n) = reader.marked().and_then(|m| m.parse().ok())
                    {
                        pid.store(n, Ordering::SeqCst);
                    }
                }
            }
        }
        let _ = done.send(reader.finish());
    });
}

/// Stops `child` and what it started: on another machine (`host`) the command there by its
/// process number, here the whole tree of processes; then the process itself.
///
/// Elsewhere, the process number may not have come yet: it is waited for a moment (the local
/// ssh alone, killed, would leave git running there). The command there is stopped before this
/// returns, so that what follows (removing a cancelled clone, the next operation) doesn't race
/// it.
fn stop(child: &mut Child, host: Option<&Host>, remote_pid: &AtomicU32) {
    match host {
        Some(host) => {
            let start = Instant::now();
            let mut pid = remote_pid.load(Ordering::SeqCst);
            while pid == 0 && start.elapsed() < PID_WAIT && matches!(child.try_wait(), Ok(None)) {
                std::thread::sleep(POLL);
                pid = remote_pid.load(Ordering::SeqCst);
            }
            if pid != 0 {
                remote::stop_now(host, pid);
            }
        }
        None => stop_here(child.id()),
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Stops the process `pid` of this machine and everything it started (git runs helpers:
/// `git-remote-https`, `ssh`), as one that hung on another machine is.
fn stop_here(pid: u32) {
    #[cfg(unix)]
    let mut cmd = {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(remote::stop_script(pid));
        cmd
    };
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    };
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::git::run;

    fn scratch(name: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-watched {name} ü {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    /// A repository with a few commits and files (names with spaces and accents among them).
    fn source(dir: &Path) -> Result<(), String> {
        run(Some(dir), &["init", "-q"])?;
        run(Some(dir), &["config", "user.name", "t"])?;
        run(Some(dir), &["config", "user.email", "t@t"])?;
        for i in 0..20 {
            let name = format!("fichier {i} é.txt");
            std::fs::write(dir.join(name), format!("{i}\n").repeat(200))
                .map_err(|e| e.to_string())?;
            run(Some(dir), &["add", "-A"])?;
            run(Some(dir), &["commit", "-q", "-m", &format!("commit {i}")])?;
        }
        Ok(())
    }

    #[test]
    fn progress_goes_after_the_subcommand_unless_told_to_be_quiet() {
        let cases: [(&[&str], &[&str]); 6] = [
            (&["fetch", "--all"], &["fetch", "--progress", "--all"]),
            (
                &["-c", "push.default=simple", "push", "-u", "o"],
                &["-c", "push.default=simple", "push", "--progress", "-u", "o"],
            ),
            (&["pull", "--quiet"], &["pull", "--quiet"]),
            (&["fetch", "-q"], &["fetch", "-q"]),
            (&["ls-remote", "o"], &["ls-remote", "o"]),
            (&[], &[]),
        ];
        for (args, want) in cases {
            assert_eq!(with_progress(args), want, "{args:?}");
        }
    }

    #[test]
    fn a_clone_reports_its_progress_and_ends() -> Result<(), String> {
        let dir = scratch("clone")?;
        let src = dir.join("source repo");
        std::fs::create_dir_all(&src).map_err(|e| e.to_string())?;
        source(&src)?;
        let dest = dir.join("copie é");
        let control = OpControl::new(|| {});
        let (src_s, dest_s) = (src.to_string_lossy(), dest.to_string_lossy());
        // `--no-local`: through git's transport, as from a server, not a copy of the files.
        let args = ["clone", "--no-local", "--", &src_s, &dest_s];
        let cloned = run_watched(None, &args, None, false, &control);
        let progress = control.progress();
        let files = std::fs::read_dir(&dest).map(|d| d.count()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&dir);
        cloned?;
        assert!(progress.is_some(), "no progress reported");
        // 20 files and `.git`.
        assert_eq!(files, 21);
        Ok(())
    }

    #[test]
    fn a_failure_says_why_without_the_progress() -> Result<(), String> {
        let dir = scratch("fails")?;
        run(Some(&dir), &["init", "-q"])?;
        let control = OpControl::new(|| {});
        let missing = dir.join("nowhere");
        let missing = missing.to_string_lossy();
        let err = run_watched(Some(&dir), &["fetch", &missing], None, false, &control);
        let _ = std::fs::remove_dir_all(&dir);
        let err = err.err().unwrap_or_default();
        assert!(
            err.contains("does not appear to be a git repository"),
            "{err}"
        );
        assert!(!err.contains('%'), "{err}");
        Ok(())
    }

    /// A cancel stops git and what it started (here the "ssh" it runs, which hangs), at once.
    #[cfg(unix)]
    #[test]
    fn a_cancel_stops_git_and_what_it_started() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("cancel")?;
        run(Some(&dir), &["init", "-q"])?;
        let pid_file = dir.join("ssh.pid");
        let ssh = dir.join("slow ssh.sh");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
                pid_file.display()
            ),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let control = OpControl::new(|| {});
        let canceller = Arc::clone(&control);
        let pid_seen = pid_file.clone();
        std::thread::spawn(move || {
            for _ in 0..200 {
                if pid_seen.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            canceller.cancel();
        });
        let ssh_config = format!("core.sshCommand='{}'", ssh.display());
        let started = Instant::now();
        let result = run_watched(
            Some(&dir),
            &["-c", &ssh_config, "fetch", "ssh://example.invalid/r.git"],
            None,
            false,
            &control,
        );
        let took = started.elapsed();
        let pid = std::fs::read_to_string(&pid_file).unwrap_or_default();
        let alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result, Err(CANCELLED.to_owned()));
        assert!(!pid.trim().is_empty(), "the fake ssh never ran");
        assert!(took < Duration::from_secs(15), "took {took:?}");
        assert!(!alive, "what git started still runs");
        Ok(())
    }

    #[test]
    fn nothing_runs_once_cancelled() -> Result<(), String> {
        let dir = scratch("cancelled-before")?;
        let control = OpControl::new(|| {});
        control.cancel();
        let result = run_watched(Some(&dir), &["fetch"], None, false, &control);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result, Err(CANCELLED.to_owned()));
        Ok(())
    }
    /// Runs the script a watched command is sent elsewhere with, here, in `dir`, as the shell
    /// there would (its variables set), reading its error output as `follow` does.
    #[cfg(unix)]
    fn run_there_here(
        dir: &Path,
        args: &[&str],
        stop_after: Option<Duration>,
    ) -> Result<(Option<i32>, Option<u32>), String> {
        let target = Target {
            host: Host::Ssh("example".to_owned()),
            path: dir.to_string_lossy().into_owned(),
        };
        let req = watched_request(&target, &Extra::default(), args);
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(&req.script)
            .current_dir(dir)
            .envs(req.env.iter().map(|(n, v)| (n.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stderr = child.stderr.take().ok_or("no stderr")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = ProgressReader::with_marker(PID_MARKER);
            let mut buf = [0u8; 512];
            while let Ok(n @ 1..) = stderr.read(&mut buf) {
                reader.feed(&buf[..n]);
                if let Some(pid) = reader.marked().and_then(|m| m.parse::<u32>().ok()) {
                    let _ = tx.send(Some(pid));
                }
            }
            let _ = tx.send(None);
        });
        let pid = rx.recv_timeout(Duration::from_secs(10)).ok().flatten();
        if let (Some(after), Some(pid)) = (stop_after, pid) {
            std::thread::sleep(after);
            std::process::Command::new("sh")
                .arg("-c")
                .arg(remote::stop_script(pid))
                .status()
                .map_err(|e| e.to_string())?;
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        Ok((status.code(), pid))
    }

    /// Elsewhere, git's process number comes first, and the process ends as git did.
    #[cfg(unix)]
    #[test]
    fn the_script_run_elsewhere_says_whom_to_stop_and_ends_as_git_did() -> Result<(), String> {
        let dir = scratch("there")?;
        run(Some(&dir), &["init", "-q"])?;
        let ok = run_there_here(&dir, &["status", "--short"], None)?;
        let failed = run_there_here(&dir, &["fetch", "/no/such/repo here"], None)?;
        // A fetch that hangs, stopped by the process number it said.
        let ssh = dir.join("hang.sh");
        std::fs::write(&ssh, "#!/bin/sh\nexec sleep 60\n").map_err(|e| e.to_string())?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let config = format!("core.sshCommand={}", ssh.display());
        let started = Instant::now();
        let stopped = run_there_here(
            &dir,
            &["-c", &config, "fetch", "ssh://example.invalid/r.git"],
            Some(Duration::from_millis(300)),
        )?;
        let took = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ok.0, Some(0));
        assert!(ok.1.is_some());
        assert_eq!(failed.0, Some(128));
        assert!(stopped.1.is_some());
        assert_ne!(stopped.0, Some(0));
        assert!(took < Duration::from_secs(15), "took {took:?}");
        Ok(())
    }
}
