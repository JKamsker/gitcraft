//! One application-owned event loop for subprocess I/O. No pipe-reader or waiter threads.
//! The synchronous facade is for the bounded blocking workers; `submit` also permits many
//! independent commands to share the event loop without a waiting caller thread.

use std::collections::VecDeque;
use std::io;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use crate::tasks::Cancellation;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(test)]
mod trace;

const ACTIVE: usize = 12;
/// Slots beyond `ACTIVE` that only urgent requests take: a click never waits for a fetch.
const URGENT_EXTRA: usize = 6;
/// How many processes may read ahead at once, and only while nothing else waits for a slot.
const SPECULATIVE: usize = 2;
const QUEUED: usize = 256;
const OUTPUT_LIMIT: usize = 64 << 20;
const POLL: Duration = Duration::from_millis(20);

pub(crate) type Filter = Box<dyn FnMut(&[u8]) -> Vec<u8> + Send>;
pub(crate) type Chunk = Box<dyn FnMut(&[u8]) + Send>;
pub(crate) struct Options {
    /// Only the packaged native endpoint uses direct-child controller ownership.
    pub native_controller: bool,
    pub input: Option<Vec<u8>>,
    /// Keep the control channel open after flushing a remote request.
    pub hold_input: bool,
    pub background: bool,
    /// Started for work the user waits on (see `tasks::urgent`): it goes ahead of the queue.
    pub urgent: bool,
    /// Started to read ahead (see `tasks::speculative`): it waits until nothing else does.
    pub speculative: bool,
    pub timeout: Option<Duration>,
    pub cancel: Cancellation,
    pub cancelled: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub stderr: Option<Chunk>,
    pub stderr_filter: Option<Filter>,
    pub stdout: Option<Chunk>,
    pub ready: Option<Box<dyn FnOnce() + Send>>,
    /// Stream stdout directly to a file instead of capturing it in memory.
    pub stdout_file: Option<std::fs::File>,
    /// Cleanup on the other machine, before returning a cancelled/timed-out operation.
    /// This is a blocking protocol API, isolated from the I/O loop on bounded workers.
    pub stop_remote: Option<Box<dyn FnOnce() -> Result<(), String> + Send>>,
    /// Best-effort removal of private artifacts, authenticated by a terminal receipt.
    pub retire_remote: Option<Box<dyn FnOnce() -> Result<(), String> + Send>>,
    /// Require delivery proof independently of target cleanup before reporting success.
    pub complete_remote: Option<Box<dyn FnOnce() -> Result<(), String> + Send>>,
    pub stop_ready: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub stdout_ready: Option<&'static str>,
    pub output_limit: usize,
    /// A smaller independent stderr cap; otherwise use the stdout capture limit.
    pub stderr_limit: Option<usize>,
    pub drain_for: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            native_controller: false,
            input: None,
            hold_input: false,
            background: false,
            urgent: crate::tasks::urgent(),
            speculative: crate::tasks::speculative(),
            timeout: None,
            cancel: crate::tasks::cancellation(),
            cancelled: None,
            stderr: None,
            stderr_filter: None,
            stdout: None,
            ready: None,
            stdout_file: None,
            stop_remote: None,
            retire_remote: None,
            complete_remote: None,
            stop_ready: None,
            stdout_ready: None,
            output_limit: OUTPUT_LIMIT,
            stderr_limit: None,
            drain_for: Duration::from_secs(1),
        }
    }
}

pub(crate) struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Partial output is retained on cancellation, timeout, I/O failure or overflow.
    pub stopped: Option<String>,
    pub stop_kind: Option<StopKind>,
    pub phases: Phases,
}
/// How long a process waited for a slot, took to start, ran until it was seen to have ended,
/// and kept its output open after that: for the timing log.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Phases {
    pub queued: Duration,
    pub spawn: Duration,
    pub ran: Duration,
    pub drained: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopKind {
    Cancelled,
    TimedOut,
    Failed,
    Uncertain,
}
fn stop_kind(reason: Option<&str>, confirmed: bool) -> Option<StopKind> {
    reason.map(|reason| {
        if !confirmed {
            StopKind::Uncertain
        } else {
            match reason {
                "cancelled" => StopKind::Cancelled,
                "process timed out and was stopped" => StopKind::TimedOut,
                _ => StopKind::Failed,
            }
        }
    })
}
fn record_stop_confirmation(
    stopped: &mut Option<String>,
    confirmed: &mut bool,
    result: Result<(), String>,
) {
    match result {
        Ok(()) => *confirmed = true,
        Err(why) => {
            *confirmed = false;
            *stopped = Some(format!(
                "{}; remote cleanup unconfirmed: {why}",
                stopped.as_deref().unwrap_or("cleanup failed")
            ));
        }
    }
}
#[cfg(test)]
mod recording_outcomes;
#[derive(Debug)]
pub(crate) enum Failure {
    CancelledBeforeStart,
    TimedOutBeforeStart,
    Failed(String),
}
impl Failure {
    pub(crate) fn message(self) -> String {
        match self {
            Self::CancelledBeforeStart => "cancelled before execution".into(),
            Self::TimedOutBeforeStart => "timed out before execution".into(),
            Self::Failed(message) => message,
        }
    }
}
pub(crate) fn checked(result: ResultOutput) -> Result<Output, Failure> {
    // These exact Err values originate only before Service dispatch. Dispatched stop or
    // timeout returns Output with typed stop_kind; arbitrary cleanup errors stay failures.
    result.map_err(|message| {
        if message == "cancelled" {
            Failure::CancelledBeforeStart
        } else if message == "process timed out and was stopped" {
            Failure::TimedOutBeforeStart
        } else {
            Failure::Failed(message)
        }
    })
}
#[cfg(test)]
pub(crate) fn recording_output(kind: Option<StopKind>) -> Output {
    #[cfg(unix)]
    let status = {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    };
    #[cfg(windows)]
    let status = {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    };
    Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stop_kind: kind,
        phases: Phases::default(),
        stopped: kind.map(|kind| {
            match kind {
                StopKind::Cancelled => "cancelled",
                StopKind::TimedOut => "process timed out and was stopped",
                StopKind::Failed => "content failure",
                StopKind::Uncertain => "cleanup proof missing",
            }
            .to_owned()
        }),
    }
}
impl Output {
    pub(crate) fn complete(self) -> Result<std::process::Output, String> {
        if let Some(reason) = self.stopped {
            return Err(reason);
        }
        Ok(std::process::Output {
            status: self.status,
            stdout: self.stdout,
            stderr: self.stderr,
        })
    }
}

type ResultOutput = Result<Output, String>;
mod service;
pub(crate) use service::*;

#[cfg(all(test, unix))]
pub(crate) fn owned_fixture_service() -> io::Result<Service> {
    Service::new(6, 16)
}

static SERVICE: OnceLock<Result<Service, String>> = OnceLock::new();
static CLEANUP: OnceLock<Result<Service, String>> = OnceLock::new();
static LEASES: OnceLock<Result<Service, String>> = OnceLock::new();
pub(crate) fn service() -> Result<&'static Service, String> {
    SERVICE
        .get_or_init(|| {
            Service::new(ACTIVE, QUEUED).map_err(|e| format!("Could not start process I/O: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub(crate) fn run(command: Command, options: Options) -> ResultOutput {
    let result = service()?.submit(command, options)?.wait();
    if let Ok(out) = &result {
        crate::timing::process(&out.phases);
    }
    result
}
/// Wait for an owned request while a snapshot's UI lifetime remains current.
/// Cancellation and cleanup stay entirely in the shared process service.
pub(crate) fn run_while(
    command: Command,
    options: Options,
    active: &dyn Fn() -> bool,
) -> ResultOutput {
    if !active() {
        return Err("Comparison cancelled".into());
    }
    let ticket = service()?.submit(command, options)?;
    loop {
        if !active() {
            ticket.cancel();
        }
        match ticket.reply.recv_timeout(POLL) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(_) => return Err("Process service stopped before returning a result".into()),
        }
    }
}
pub(crate) fn run_checked(command: Command, options: Options) -> Result<Output, Failure> {
    checked(run(command, options))
}
/// Dedicated bounded cleanup capacity cannot deadlock behind the transports whose
/// owners are waiting for this acknowledgement. It never inherits caller cancellation.
pub(crate) fn run_cleanup(command: Command, mut options: Options) -> ResultOutput {
    options.cancel = Cancellation::default();
    let service = CLEANUP.get_or_init(|| Service::new(2, 32).map_err(|e| e.to_string()));
    service
        .as_ref()
        .map_err(Clone::clone)?
        .submit(command, options)?
        .wait()
}

/// Claim-only helpers have independent bounded capacity; they never run repository code.
pub(crate) fn submit_lease(command: Command, options: Options) -> Result<Ticket, String> {
    LEASES
        .get_or_init(|| Service::new(16, 64).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(Clone::clone)?
        .submit(command, options)
}
pub(crate) fn shutdown_leases() {
    if let Some(Ok(service)) = LEASES.get() {
        service.shutdown();
    }
}

pub(crate) fn counts() -> Option<Counts> {
    SERVICE
        .get()
        .and_then(|s| s.as_ref().ok())
        .map(Service::counts)
}
pub(crate) fn shutdown() {
    if let Some(Ok(service)) = SERVICE.get() {
        service.shutdown();
    }
}

pub(crate) fn shutdown_cleanup() {
    if let Some(Ok(service)) = CLEANUP.get() {
        service.shutdown();
    }
}

fn before_start(cancelled: bool, expired: bool) -> Option<&'static str> {
    if cancelled {
        Some("cancelled")
    } else if expired {
        Some("process timed out and was stopped")
    } else {
        None
    }
}

mod execution;
use execution::*;

#[cfg(test)]
mod tests;
