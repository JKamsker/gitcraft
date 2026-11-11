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
    /// Finite Gerrit witness owned by this operation, never a log-output cache.
    stack_failure: Mutex<Option<crate::gerrit::StackFailure>>,
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
            stack_failure: Mutex::new(None),
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

    pub(crate) fn stack_checked(&self, source: Option<crate::gerrit::StackFailure>) {
        if let Ok(mut stored) = self.stack_failure.lock() {
            *stored = source;
        }
    }

    pub(crate) fn stack_failure(&self) -> Option<crate::gerrit::StackFailure> {
        self.stack_failure.lock().ok().and_then(|s| s.clone())
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
        let not_run = Recorded::new(args, None, "Not run: cancelled", creds, started);
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
    if ended.code != Some(0)
        && failed_hook
            .as_ref()
            .is_some_and(|hook| hook.name == "pre-push")
    {
        // Keep the same privacy boundary as reviewed pushes, including the activity
        // and hook logs: arbitrary local hook output can contain credentials.
        ended.stdout.clear();
        ended.stderr = "Hook output withheld because it can contain credentials or secrets.".into();
    }
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
        let e = super::missing_folder(t);
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
mod tests;
