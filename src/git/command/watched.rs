//! Running a git command that goes over the network (fetch, pull, push, clone) so that it can
//! be followed and stopped. Both local execution and SSH/WSL transports use the shared
//! process service. Remote commands establish an authenticated owner before Git runs;
//! cancellation waits for stop/reap acknowledgement before retiring the local transport.

use std::path::Path;
#[cfg(all(test, unix))]
use std::process::Stdio;
use std::sync::{Arc, Mutex};
#[cfg(all(test, unix))]
use std::time::Duration;
use std::time::Instant;

use super::{
    Credentials, Extra, Recorded, after_options, extra_for, failure_message, local_command,
};
use crate::git::progress::{Progress, ProgressReader, without_progress};
#[cfg(all(test, unix))]
use crate::remote::Host;
use crate::remote::{self, Target};

/// What a cancelled command fails with.
pub const CANCELLED: &str = "cancelled";

/// How the UI follows a running operation and stops it: shared with the thread that runs it.
pub struct OpControl {
    cancel: crate::tasks::Cancellation,
    progress: Mutex<Option<Progress>>,
    /// What each git call of the operation did so far, for the activity log.
    steps: Mutex<Vec<Recorded>>,
    auth_failure: Mutex<Option<super::AuthFailure>>,
    /// Called whenever there is news (a repaint of the window).
    notify: Box<dyn Fn() + Send + Sync>,
}

impl OpControl {
    /// A control that calls `notify` when the progress moves.
    pub fn new(notify: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Self::scoped(notify, crate::tasks::Cancellation::default())
    }

    pub(crate) fn scoped(
        notify: impl Fn() + Send + Sync + 'static,
        cancel: crate::tasks::Cancellation,
    ) -> Arc<Self> {
        Arc::new(Self {
            cancel,
            progress: Mutex::new(None),
            steps: Mutex::new(Vec::new()),
            auth_failure: Mutex::new(None),
            notify: Box::new(notify),
        })
    }

    /// Asks the operation to stop: the command running is stopped, and no step after it runs.
    pub fn cancel(&self) {
        self.cancel.cancel();
        crate::tasks::purge_cancelled();
        (self.notify)();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub(crate) fn cancellation(&self) -> crate::tasks::Cancellation {
        self.cancel.clone()
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

    pub(crate) fn authentication_failed(&self, failure: super::AuthFailure) {
        if let Ok(mut stored) = self.auth_failure.lock() {
            *stored = Some(failure);
        }
    }
    pub(crate) fn auth_failure(&self) -> Option<super::AuthFailure> {
        self.auth_failure.lock().ok().and_then(|s| s.clone())
    }

    /// What the operation's git calls did, taken out (once it has ended).
    pub fn take_steps(&self) -> Vec<Recorded> {
        self.steps
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default()
    }

    /// The git calls made so far, without their output: their arguments and exit codes.
    pub fn step_summaries(&self) -> Vec<(Vec<String>, Option<i32>)> {
        self.steps
            .lock()
            .map(|s| s.iter().map(|r| (r.args.clone(), r.code)).collect())
            .unwrap_or_default()
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

/// How a followed command ended.
struct Ended {
    stopped: Option<String>,
    code: Option<i32>,
    status: String,
    stdout: String,
    stderr: String,
}

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
    let extra = match extra_for(dir, args, creds, quiet) {
        Ok(extra) => extra,
        Err(e) => {
            return (
                Err(e.clone()),
                Recorded::new(args, None, &e, creds, Instant::now()),
            );
        }
    };
    run_watched_extra_recorded(dir, args, creds, quiet, control, extra)
}

pub(in crate::git) fn run_watched_extra_recorded(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
    control: &Arc<OpControl>,
    mut extra: Extra,
) -> (Result<String, String>, Recorded) {
    let started = Instant::now();
    if control.is_cancelled() {
        let not_run = Recorded::new(
            args,
            None,
            crate::i18n::text("backend-not-run-cancelled-02b0ef"),
            creds,
            started,
        );
        return (Err(CANCELLED.to_owned()), not_run);
    }
    let _timer = crate::git::backend::CliTimer::new(args);
    let words = if quiet {
        args.iter().map(|a| (*a).to_owned()).collect()
    } else {
        with_progress(args)
    };
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let failed = |e: String| {
        let rec = Recorded::new(&words, None, &e, creds, started);
        (Err(e), rec)
    };
    let _source = match super::source::bind(&mut extra, dir, args) {
        Ok(source) => source,
        Err(e) => return failed(e),
    };
    extra.trace_hooks = !quiet && crate::git::hooks::wants_trace(args);
    let trace = (dir.and_then(remote::parse).is_none() && extra.trace_hooks)
        .then(crate::git::hooks::TraceFile::new)
        .flatten();
    if let Some(trace) = &trace {
        extra.env.push((
            crate::git::hooks::TRACE_VAR,
            trace.path().to_string_lossy().into_owned(),
        ));
    }
    let target = dir.and_then(remote::parse);
    let mut ended = match follow_command(dir, &extra, &words, target.as_ref(), control) {
        Ok(ended) => ended,
        Err(e) => return failed(e),
    };
    let failed_hook = if target.is_some() && extra.trace_hooks {
        let (stderr, events) = crate::git::hooks::split_trace(ended.stderr.as_bytes());
        ended.stderr = String::from_utf8_lossy(&stderr).into_owned();
        crate::git::hooks::failed_hook(&events, args)
    } else {
        trace.as_ref().and_then(|t| t.failed_hook(args))
    };
    crate::git::doctor::hook_log::record(dir, args, ended.code == Some(0), ended.stderr.as_bytes());
    let recorded = Recorded::new(
        &words,
        ended.code,
        &super::both(&ended.stdout, &ended.stderr),
        creds,
        started,
    );
    if let Some(reason) = ended.stopped {
        return (Err(reason), recorded);
    }
    if ended.code == Some(0) {
        if let Err(e) = super::source::finish(_source.as_ref(), args, true) {
            return (Err(e), recorded);
        }
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
        let e = crate::i18n::message(
            "backend-isn-t-a-folder-on-859699",
            &[
                ("path", (t.path).to_string().into()),
                ("t", (t.host.label()).to_string().into()),
            ],
        );
        return (Err(e), recorded);
    }
    let e = failure_message(
        args,
        &ended.status,
        &ended.stdout,
        &without_progress(&ended.stderr),
    );
    let e = failed_hook.map_or_else(|| e.clone(), |hook| hook.explain(&e, args));
    (Err(e), recorded)
}

/// Reviewed target/options use the same progress, cancellation and owned transport as normal pushes.
pub(super) fn reviewed(
    dir: &Path,
    extra: &Extra,
    args: &[&str],
    control: &Arc<OpControl>,
) -> Result<super::Ran, String> {
    let started = Instant::now();
    if control.is_cancelled() {
        return Err(CANCELLED.into());
    }
    let words = with_progress(args);
    let words = words.iter().map(String::as_str).collect::<Vec<_>>();
    let target = remote::parse(dir);
    let ended = follow_command(Some(dir), extra, &words, target.as_ref(), control)?;
    control.record(Recorded::new(
        &words,
        ended.code,
        if ended.code == Some(0) {
            crate::i18n::text("backend-reviewed-push-completed-f39141")
        } else {
            crate::i18n::text("backend-reviewed-push-failed-server-diagnostics-withheld-2b8b01")
        },
        None,
        started,
    ));
    if let Some(reason) = ended.stopped {
        return Err(reason);
    }
    Ok(super::Ran {
        code: ended.code,
        status: ended.status,
        stdout: ended.stdout.into_bytes(),
        stderr: ended.stderr.into_bytes(),
        failed_hook: None,
    })
}

/// Drains both pipes and writes input on the process service's single event loop. Only
/// the existing synchronous caller waits, on a bounded background worker.
fn follow_command(
    dir: Option<&Path>,
    extra: &Extra,
    args: &[&str],
    target: Option<&Target>,
    control: &Arc<OpControl>,
) -> Result<Ended, String> {
    let reader = Arc::new(Mutex::new(ProgressReader::default()));
    let mut options = crate::processes::Options {
        background: true,
        timeout: extra.timeout,
        output_limit: extra.output_limit.unwrap_or(64 << 20),
        ..Default::default()
    };
    let command = match target {
        Some(target) => {
            let streaming = remote::streaming_command(
                &target.host,
                &super::remote_request(target, extra, args),
            )?;
            options.timeout = extra.timeout.or(Some(remote::NETWORK_LIMIT));
            streaming.configure(&mut options)
        }
        None => local_command(dir, extra, args),
    };
    let followed = Arc::clone(control);
    options.cancelled = Some(Box::new(move || followed.is_cancelled()));
    let followed = Arc::clone(control);
    let reading = Arc::clone(&reader);
    options.stderr = Some(Box::new(move |chunk| {
        let mut reader = reading
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if reader.feed(chunk) {
            followed.set_progress(reader.latest().cloned());
        }
    }));
    let out = crate::processes::run(command, options)?;
    let stderr = std::mem::take(
        &mut *reader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
    .finish();
    Ok(Ended {
        stopped: out.stopped,
        code: out.status.code(),
        status: out.status.to_string(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr,
    })
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
        let canceller_thread = std::thread::spawn(move || {
            for _ in 0..200 {
                if std::fs::read_to_string(&pid_seen).ok().is_some_and(|pid| {
                    pid.ends_with('\n') && pid.trim().parse::<u32>().is_ok_and(|pid| pid > 0)
                }) {
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
        canceller_thread.join().map_err(|_| "canceller panicked")?;
        let pid = std::fs::read_to_string(&pid_file).unwrap_or_default();
        let pid = pid.trim();
        assert_eq!(result, Err(CANCELLED.to_owned()));
        assert!(!pid.is_empty(), "the fake ssh never ran");
        assert!(took < Duration::from_secs(15), "took {took:?}");
        let running = || -> Result<bool, String> {
            // One /proc snapshot avoids a kill-0/read-stat race: the process can be
            // reaped between those calls. A container's PID 1 may also leave zombies,
            // which have stopped executing and must not count as surviving helpers.
            #[cfg(target_os = "linux")]
            match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(stat) => {
                    return Ok(!stat.rsplit_once(") ").is_some_and(|(_, rest)| {
                        rest.starts_with("Z ") || rest.starts_with("X ")
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(_) => {}
            }
            std::process::Command::new("kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .map_err(|error| error.to_string())
        };
        // SIGKILL delivery and waiting for Git do not synchronize the scheduler's exit
        // transition of its nonchild helper. Still require that helper to stop promptly:
        // an escaped sleep would continue for 60 seconds and fail this bounded check.
        let deadline = Instant::now() + Duration::from_secs(2);
        let alive = loop {
            let alive = running()?;
            if !alive || Instant::now() >= deadline {
                break alive;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let state = if alive {
            std::process::Command::new("ps")
                .args(["-o", "pid=,ppid=,pgid=,stat=,command=", "-p", pid])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!alive, "what git started still runs: {state}");
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
    /// Exercise the same owned streaming dispatcher used by watched Git.
    #[cfg(unix)]
    fn run_there_here(
        dir: &Path,
        args: &[&str],
        stop_after: Option<Duration>,
    ) -> Result<(Option<i32>, bool), String> {
        let target = Target {
            host: Host::Ssh(remote::LOOPBACK.into()),
            path: dir.to_string_lossy().into_owned(),
        };
        let req = super::super::remote_request(&target, &Extra::default(), args);
        let streaming = remote::streaming_command(&target.host, &req)?;
        let lease = streaming.lease.clone();
        let token = crate::tasks::Cancellation::default();
        let stop = token.clone();
        let canceller = stop_after.map(|after| {
            std::thread::spawn(move || {
                std::thread::sleep(after);
                stop.cancel();
            })
        });
        let mut options = crate::processes::Options {
            cancel: token,
            timeout: Some(Duration::from_secs(15)),
            ..Default::default()
        };
        let command = streaming.configure(&mut options);
        let result = crate::processes::run(command, options)?;
        if let Some(canceller) = canceller {
            canceller.join().map_err(|_| "canceller panicked")?;
        }
        if let Some(reason) = &result.stopped {
            assert!(!reason.contains("unconfirmed"), "{reason}");
        }
        Ok((result.status.code(), lease.endpoint().is_some()))
    }

    #[cfg(unix)]
    #[test]
    fn the_script_run_elsewhere_has_an_owner_and_ends_as_git_did() -> Result<(), String> {
        let dir = scratch("there")?;
        run(Some(&dir), &["init", "-q"])?;
        let ok = run_there_here(&dir, &["status", "--short"], None)?;
        let failed = run_there_here(&dir, &["fetch", "/no/such/repo here"], None)?;
        let stopped = run_there_here(
            &dir,
            &["-c", "alias.wait=!sleep 60", "wait"],
            Some(Duration::from_millis(300)),
        )?;
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        assert_eq!(ok, (Some(0), true));
        assert_eq!(failed, (Some(128), true));
        assert!(stopped.1);
        assert_ne!(stopped.0, Some(0));
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn remote_stream_progress_cancel_and_partial_output_use_the_event_loop() -> Result<(), String> {
        let dir = scratch("async-remote")?;
        let repo = std::path::PathBuf::from(format!("ssh://{}{}", remote::LOOPBACK, dir.display()));
        let control = OpControl::new(|| {});
        let cancel = Arc::clone(&control);
        let canceller = std::thread::spawn(move || {
            let start = Instant::now();
            while cancel.progress().is_none() && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let progress = cancel.progress();
            cancel.cancel();
            progress
        });
        let (result, recorded) = run_watched_recorded(
            Some(&repo),
            &[
                "-c",
                "alias.stream=!printf partial-output; printf 'Receiving objects: 42%% (42/100)\\r' >&2; sleep 60",
                "stream",
            ],
            None,
            false,
            &control,
        );
        let progress = canceller.join().map_err(|_| "canceller panicked")?;
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(result, Err(CANCELLED.into()));
        assert_eq!(progress.and_then(|p| p.percent), Some(42));
        assert!(
            recorded.output.contains("partial-output"),
            "{}",
            recorded.output
        );
        Ok(())
    }
}
