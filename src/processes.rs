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

const ACTIVE: usize = 8;
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
#[cfg(test)]
mod recording_outcomes {
    use super::*;
    #[test]
    fn cancellation_requires_no_dispatch_or_confirmed_cleanup() {
        assert!(matches!(
            checked(Err("cancelled".into())),
            Err(Failure::CancelledBeforeStart)
        ));
        assert!(matches!(
            checked(Err("could not finish owned process".into())),
            Err(Failure::Failed(_))
        ));
        assert!(matches!(
            checked(Err(
                "Process service stopped before returning a result".into()
            )),
            Err(Failure::Failed(_))
        ));
        assert_eq!(
            stop_kind(Some("cancelled"), false),
            Some(StopKind::Uncertain)
        );
        assert_eq!(
            stop_kind(Some("cancelled"), true),
            Some(StopKind::Cancelled)
        );
        assert_eq!(
            stop_kind(Some("output delivery incomplete"), false),
            Some(StopKind::Uncertain)
        );
    }
}
#[derive(Debug)]
pub(crate) enum Failure {
    CancelledBeforeStart,
    Failed(String),
}
impl Failure {
    pub(crate) fn message(self) -> String {
        match self {
            Self::CancelledBeforeStart => "cancelled before execution".into(),
            Self::Failed(message) => message,
        }
    }
}
pub(crate) fn checked(result: ResultOutput) -> Result<Output, Failure> {
    result.map_err(|message| {
        if message == "cancelled" {
            Failure::CancelledBeforeStart
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
struct Request {
    command: Command,
    options: Options,
    reply: mpsc::Sender<ResultOutput>,
    ticket: Cancellation,
    deadline: Option<Instant>,
    #[cfg(test)]
    trace: Option<Arc<trace::Trace>>,
}

pub(crate) struct Ticket {
    reply: mpsc::Receiver<ResultOutput>,
    cancel: Cancellation,
    #[cfg(test)]
    trace: Option<Arc<trace::Trace>>,
}
impl Ticket {
    pub(crate) fn wait(self) -> ResultOutput {
        #[cfg(test)]
        if let Some(trace) = &self.trace {
            loop {
                match self.reply.recv_timeout(Duration::from_secs(30)) {
                    Ok(result) => return result,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("Process service stopped before returning a result".into());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => trace.report(),
                }
            }
        }
        self.reply
            .recv()
            .map_err(|_| "Process service stopped before returning a result".to_owned())?
    }
    pub(crate) fn try_result(&self) -> Option<ResultOutput> {
        match self.reply.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Process service stopped before returning a result".into(),
            )),
        }
    }
    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Default)]
struct Counters {
    active: AtomicUsize,
    queued: AtomicUsize,
    spawned: AtomicUsize,
    peak: AtomicUsize,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct Counts {
    pub active: usize,
    pub queued: usize,
    pub spawned: usize,
    pub peak: usize,
}

pub(crate) struct Service {
    sender: tokio::sync::mpsc::Sender<Request>,
    stop: Cancellation,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    counters: Arc<Counters>,
    capacity: usize,
}
impl Service {
    fn new(active: usize, queued: usize) -> io::Result<Self> {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Request>(queued);
        let stop = Cancellation::default();
        let shutdown = stop.clone();
        let counters = Arc::new(Counters::default());
        let metrics = Arc::clone(&counters);
        // Build before starting the thread, so runtime failures reach the caller.
        // Tokio's Windows anonymous pipes use blocking I/O internally. Each active child
        // needs room for both readers and stdin, plus remote cleanup. The bound follows
        // active children, never the number of queued requests.
        let blocking_threads = if cfg!(windows) { 3 * active + 2 } else { 2 };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(blocking_threads)
            .build()?;
        let thread = std::thread::Builder::new()
            .name("gitcraft-process-io".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let blocking = Arc::new(tokio::sync::Semaphore::new(2));
                    let mut running = tokio::task::JoinSet::new();
                    let mut pending: VecDeque<Request> = VecDeque::new();
                    let mut background = 0usize;
                    let background_limit = active.saturating_sub(2).max(1);
                    let mut tick = tokio::time::interval(POLL);
                    loop {
                        if shutdown.is_cancelled() {
                            break;
                        }
                        pending.retain(|request| {
                            let cancelled = request.ticket.is_cancelled()
                                || request.options.cancel.is_cancelled()
                                || request.options.cancelled.as_ref().is_some_and(|f| f());
                            let expired = request.deadline.is_some_and(|at| Instant::now() >= at);
                            if cancelled || expired {
                                metrics.queued.fetch_sub(1, Ordering::Relaxed);
                                let _ = request.reply.send(Err(if cancelled {
                                    "cancelled"
                                } else {
                                    "process timed out and was stopped"
                                }
                                .into()));
                            }
                            !cancelled && !expired
                        });
                        while running.len() < active {
                            // Reserve two slots for short reads; dispatch the oldest eligible
                            // request so a stream of reads cannot starve long operations.
                            let selected = pending.iter().position(|r| {
                                !r.options.background || background < background_limit
                            });
                            let Some(request) = selected.and_then(|i| pending.remove(i)) else {
                                break;
                            };
                            metrics.queued.fetch_sub(1, Ordering::Relaxed);
                            let is_background = request.options.background;
                            if is_background {
                                background += 1;
                            }
                            let stats = Arc::clone(&metrics);
                            let stop = shutdown.clone();
                            let blocking = Arc::clone(&blocking);
                            running.spawn(async move {
                                let n = stats.active.fetch_add(1, Ordering::Relaxed) + 1;
                                stats.peak.fetch_max(n, Ordering::Relaxed);
                                let result = execute(
                                    request.command,
                                    request.options,
                                    request.deadline,
                                    &request.ticket,
                                    &stop,
                                    &stats,
                                    &blocking,
                                    #[cfg(test)]
                                    request.trace.as_deref(),
                                )
                                .await;
                                stats.active.fetch_sub(1, Ordering::Relaxed);
                                let _ = request.reply.send(result);
                                is_background
                            });
                        }
                        tokio::select! {
                            _ = tick.tick() => {}
                            Some(request) = receiver.recv() => pending.push_back(request),
                            Some(result) = running.join_next(), if !running.is_empty() => {
                                if matches!(result, Ok(true)) { background -= 1; }
                            }
                        }
                    }
                    receiver.close();
                    for request in pending {
                        metrics.queued.fetch_sub(1, Ordering::Relaxed);
                        let _ = request.reply.send(Err("cancelled".into()));
                    }
                    while let Some(request) = receiver.recv().await {
                        metrics.queued.fetch_sub(1, Ordering::Relaxed);
                        let _ = request.reply.send(Err("cancelled".into()));
                    }
                    // Never abort tasks: each kills and reaps its child before completing.
                    while running.join_next().await.is_some() {}
                });
            })?;
        Ok(Self {
            sender,
            stop,
            thread: Mutex::new(Some(thread)),
            counters,
            capacity: queued,
        })
    }

    pub(crate) fn submit(&self, command: Command, options: Options) -> Result<Ticket, String> {
        if self.stop.is_cancelled() {
            return Err("Process service has shut down".into());
        }
        let (reply, receive) = mpsc::channel();
        let ticket = Cancellation::default();
        let deadline = options.timeout.map(|limit| Instant::now() + limit);
        #[cfg(test)]
        let trace = trace::Trace::new(&command).map(Arc::new);
        let mut queued = self.counters.queued.load(Ordering::Relaxed);
        loop {
            if queued >= self.capacity {
                return Err("Process queue is full; try again when current work finishes".into());
            }
            match self.counters.queued.compare_exchange_weak(
                queued,
                queued + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => queued = actual,
            }
        }
        if self
            .sender
            .try_send(Request {
                command,
                options,
                reply,
                ticket: ticket.clone(),
                deadline,
                #[cfg(test)]
                trace: trace.clone(),
            })
            .is_err()
        {
            self.counters.queued.fetch_sub(1, Ordering::Relaxed);
            return Err("Process queue is full; try again when current work finishes".into());
        }
        Ok(Ticket {
            reply: receive,
            cancel: ticket,
            #[cfg(test)]
            trace,
        })
    }

    pub(crate) fn counts(&self) -> Counts {
        Counts {
            active: self.counters.active.load(Ordering::Relaxed),
            queued: self.counters.queued.load(Ordering::Relaxed),
            spawned: self.counters.spawned.load(Ordering::Relaxed),
            peak: self.counters.peak.load(Ordering::Relaxed),
        }
    }

    fn shutdown(&self) {
        self.stop.cancel();
        if let Some(thread) = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = thread.join();
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}

static SERVICE: OnceLock<Result<Service, String>> = OnceLock::new();
static CLEANUP: OnceLock<Result<Service, String>> = OnceLock::new();
pub(crate) fn service() -> Result<&'static Service, String> {
    SERVICE
        .get_or_init(|| {
            Service::new(ACTIVE, QUEUED).map_err(|e| format!("Could not start process I/O: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub(crate) fn run(command: Command, options: Options) -> ResultOutput {
    service()?.submit(command, options)?.wait()
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

// The eighth argument only exists in opt-in libtest diagnostics.
#[cfg_attr(test, allow(clippy::too_many_arguments))]
async fn execute(
    mut command: Command,
    mut options: Options,
    deadline: Option<Instant>,
    ticket: &Cancellation,
    shutdown: &Cancellation,
    stats: &Counters,
    blocking: &Arc<tokio::sync::Semaphore>,
    #[cfg(test)] trace: Option<&trace::Trace>,
) -> ResultOutput {
    let cancelled = || {
        options.cancel.is_cancelled()
            || ticket.is_cancelled()
            || shutdown.is_cancelled()
            || options.cancelled.as_ref().is_some_and(|f| f())
    };
    if cancelled() {
        return Err("cancelled".into());
    }
    if deadline.is_some_and(|at| Instant::now() >= at) {
        return Err("process timed out and was stopped".into());
    }
    command
        .stdin(if options.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(
            options
                .stdout_file
                .take()
                .map_or_else(Stdio::piped, Stdio::from),
        )
        .stderr(Stdio::piped());
    #[cfg(test)]
    if let Some(trace) = trace {
        trace.stage(trace::Stage::Spawning);
    }
    let mut child = if options.native_controller {
        gitcraft_os::OwnedChild::spawn_controller(command)
    } else {
        gitcraft_os::OwnedChild::spawn(command)
    }
    .map_err(|e| format!("could not run process: {e}"))?;
    #[cfg(test)]
    if let Some(trace) = trace {
        trace.spawned(Some(child.id()));
        trace.stage(trace::Stage::Supervising);
    }
    stats.spawned.fetch_add(1, Ordering::Relaxed);
    #[cfg(test)]
    if let Some(trace) = trace {
        trace.stage(trace::Stage::Running);
    }
    let mut input = child
        .stdin()
        .map(tokio::process::ChildStdin::from_std)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut out = child
        .stdout()
        .map(tokio::process::ChildStdout::from_std)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut err = child
        .stderr()
        .map(tokio::process::ChildStderr::from_std)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut preamble = Vec::new();
    let mut ready = options.stdout_ready.is_none();
    let mut written = 0;
    let mut status = None;
    let mut stopped = None;
    let mut drain_deadline = None;
    let mut stop_requested: Option<(String, Instant)> = None;
    let mut tick = tokio::time::interval(POLL);
    let mut obuf = [0; 8192];
    let mut ebuf = [0; 8192];
    loop {
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.io(
                input.is_some(),
                out.is_some(),
                err.is_some(),
                status.is_some(),
                written,
                stdout.len(),
                stderr.len(),
            );
        }
        if status.is_some() && out.is_none() && err.is_none() {
            break;
        }
        tokio::select! {
            result = async { match &mut out { Some(p) => p.read(&mut obuf).await, None => std::future::pending().await } } => {
                match result {
                    Ok(0) => out = None,
                    Ok(n) => {
                        if ready {
                            consume(&mut options.stdout, &obuf[..n], &mut stopped);
                            append(&mut stdout, &obuf[..n], options.output_limit, &mut stopped);
                        }
                        else {
                            for (i, &byte) in obuf[..n].iter().enumerate() {
                                if byte == b'\n' {
                                    if preamble.trim_ascii() == options.stdout_ready.unwrap_or_default().as_bytes() {
                                        ready = true;
                                        if let Some(connected) = options.ready.take() {
                                            // Saving a credential can block; it must not occupy the I/O loop.
                                            let blocking = Arc::clone(blocking);
                                            tokio::spawn(async move {
                                                if let Ok(_permit) = blocking.acquire_owned().await {
                                                    let _ = tokio::task::spawn_blocking(connected).await;
                                                }
                                            });
                                        }
                                        consume(&mut options.stdout, &obuf[i+1..n], &mut stopped);
                                        append(&mut stdout, &obuf[i+1..n], options.output_limit, &mut stopped);
                                        break;
                                    }
                                    preamble.clear();
                                } else if preamble.len() < 65536 { preamble.push(byte); }
                            }
                        }
                    }
                    Err(e) => stopped = Some(format!("could not read process output: {e}")),
                }
            }
            result = async { match &mut err { Some(p) => p.read(&mut ebuf).await, None => std::future::pending().await } } => {
                match result {
                    Ok(0) => err = None,
                    Ok(n) => {
                        let filtered = options.stderr_filter.as_mut().map(|filter| filter(&ebuf[..n]));
                        let bytes = filtered.as_deref().unwrap_or(&ebuf[..n]);
                        if let Some(consume) = &mut options.stderr
                            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| consume(bytes))).is_err() {
                            stopped = Some("process progress callback panicked".into());
                        }
                        append(&mut stderr, bytes, options.stderr_limit.unwrap_or(options.output_limit), &mut stopped);
                    }
                    Err(e) => stopped = Some(format!("could not read process errors: {e}")),
                }
            }
            result = async {
                match &mut input {
                    Some(_) if options.hold_input && written == usize::MAX => std::future::pending().await,
                    Some(pipe) => write_input(pipe, options.input.as_deref().unwrap_or_default(), &mut written).await,
                    None => std::future::pending().await,
                }
            } => {
                match result {
                    Ok(true) => { if options.hold_input { written = usize::MAX; } else { input = None; } },
                    Ok(false) => {},
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => input = None,
                    Err(e) => stopped = Some(format!("could not pass process its input: {e}")),
                }
            }
            _ = tick.tick() => {
                if status.is_none() {
                    match child.observe() {
                        Ok(Some(s)) => { status = Some(s); input = None; drain_deadline = Some(Instant::now() + options.drain_for); }
                        Ok(None) => {},
                        Err(e) => stopped = Some(format!("could not observe process: {e}")),
                    }
                }
                if cancelled() && status.is_none() { stop_requested.get_or_insert_with(|| ("cancelled".into(), Instant::now())); }
                else if status.is_none() && deadline.is_some_and(|at| Instant::now() >= at) {
                    stop_requested.get_or_insert_with(|| ("process timed out and was stopped".into(), Instant::now()));
                } else if drain_deadline.is_some_and(|until| Instant::now() >= until) {
                    stopped = Some("process ended, but something it started kept its output open".into());
                }
            }
        }
        if let Some((reason, since)) = &stop_requested
            && (options.stop_ready.as_ref().is_none_or(|ready| ready())
                || since.elapsed() >= Duration::from_secs(5))
        {
            stopped = Some(reason.clone());
        }
        if stopped.is_some() {
            break;
        }
    }
    let mut remote_confirmed = stopped.is_none() || options.stop_remote.is_none();
    if stopped.is_some() {
        // A native helper owns another group/Job. Deliver controller EOF before control
        // recovery; an unconfirmed helper is retained for reaping, never killed here.
        if options.native_controller {
            drop(input.take());
        }
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Stopping);
        }
        // The stderr filter has consumed every ownership handshake chunk read so far.
        // Authenticate stop/reap off the event loop, before ending the local transport.
        if let Some(stop) = options.stop_remote.take()
            && let Ok(_permit) = Arc::clone(blocking).acquire_owned().await
        {
            match tokio::task::spawn_blocking(stop).await {
                Ok(Ok(())) => remote_confirmed = true,
                result => {
                    let why = match result {
                        Ok(Err(why)) => why,
                        Err(e) => e.to_string(),
                        _ => String::new(),
                    };
                    stopped = Some(format!(
                        "{}; remote cleanup unconfirmed: {why}",
                        stopped.as_deref().unwrap_or_default()
                    ));
                }
            }
        }
        // finish signals while the Unix leader is unreaped, then reaps exactly once;
        // Windows holds its Job through the final signal. No delayed numeric kills.
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Reaping);
        }
        match child.finish() {
            Ok(s) => {
                if status.is_none() {
                    status = Some(s);
                }
            }
            Err(e) => {
                return Err(format!(
                    "{}; could not finish owned process: {e}",
                    stopped.as_deref().unwrap_or("cleanup failed")
                ));
            }
        }
        // Killing/reaping does not consume bytes already buffered in the pipes. Retain
        // those diagnostics without waiting indefinitely for an inherited pipe to close.
        let until = tokio::time::Instant::now() + options.drain_for;
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Draining);
        }
        let mut overflow = None;
        while out.is_some() || err.is_some() {
            tokio::select! {
                result = async { match &mut out { Some(p) => p.read(&mut obuf).await, None => std::future::pending().await } } => {
                    match result {
                        Ok(0) | Err(_) => out = None,
                        Ok(n) => if ready {
                            consume(&mut options.stdout, &obuf[..n], &mut overflow);
                            append(&mut stdout, &obuf[..n], options.output_limit, &mut overflow);
                        },
                    }
                }
                result = async { match &mut err { Some(p) => p.read(&mut ebuf).await, None => std::future::pending().await } } => {
                    match result {
                        Ok(0) | Err(_) => err = None,
                        Ok(n) => {
                            let filtered = options.stderr_filter.as_mut().map(|filter| filter(&ebuf[..n]));
                            let bytes = filtered.as_deref().unwrap_or(&ebuf[..n]);
                            if let Some(consume) = &mut options.stderr {
                                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| consume(bytes)));
                            }
                            append(&mut stderr, bytes, options.stderr_limit.unwrap_or(options.output_limit), &mut overflow);
                        }
                    }
                }
                _ = tokio::time::sleep_until(until) => break,
            }
        }
    }
    // Also retire successful owners: descendants with closed output remain in the group.
    // This is idempotent when cancellation already finished it above.
    child
        .finish()
        .map_err(|e| format!("could not finish owned process: {e}"))?;
    if stopped.is_none()
        && let Some(confirm) = options.complete_remote.take()
    {
        match Arc::clone(blocking).acquire_owned().await {
            Ok(_permit) => match tokio::task::spawn_blocking(confirm).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    stopped = Some(error);
                    remote_confirmed = false;
                }
                Err(error) => {
                    stopped = Some(format!("output delivery confirmation failed: {error}"));
                    remote_confirmed = false;
                }
            },
            Err(error) => {
                stopped = Some(format!("output delivery confirmation unavailable: {error}"));
                remote_confirmed = false;
            }
        }
    }
    if remote_confirmed
        && let Some(retire) = options.retire_remote.take()
        && let Ok(_permit) = Arc::clone(blocking).acquire_owned().await
    {
        // The endpoint itself requires its atomic receipt before unlinking anything.
        // Failed retirement leaves private evidence; it never authorizes further signals.
        let _ = tokio::task::spawn_blocking(retire).await;
    }
    if !ready && stopped.is_none() {
        let said = String::from_utf8_lossy(&stderr);
        stopped = Some(if said.trim().is_empty() {
            "connection closed before the process was ready".into()
        } else {
            format!("could not reach remote process: {}", said.trim())
        });
    }
    Ok(Output {
        status: status.ok_or("process ended without an exit status")?,
        stdout,
        stderr,
        stop_kind: stop_kind(stopped.as_deref(), remote_confirmed),
        stopped,
    })
}

/// Windows pipe writes can report acceptance before their blocking I/O finishes. Keep
/// polling that final write through flush before dropping the handle to deliver EOF,
/// while the child is alive. Child exit or BrokenPipe can still end input early.
/// Each await stays inside the main select so cancellation and deadlines still advance.
async fn write_input<W: tokio::io::AsyncWrite + Unpin>(
    pipe: &mut W,
    bytes: &[u8],
    written: &mut usize,
) -> io::Result<bool> {
    if *written < bytes.len() {
        let n = pipe.write(&bytes[*written..]).await?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        *written += n;
        Ok(false)
    } else {
        pipe.flush().await?;
        Ok(true)
    }
}

fn consume(callback: &mut Option<Chunk>, bytes: &[u8], stopped: &mut Option<String>) {
    if let Some(callback) = callback
        && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(bytes))).is_err()
    {
        *stopped = Some("process output callback panicked".into());
    }
}

fn append(output: &mut Vec<u8>, bytes: &[u8], limit: usize, stopped: &mut Option<String>) {
    let keep = bytes.len().min(limit.saturating_sub(output.len()));
    output.extend_from_slice(&bytes[..keep]);
    if keep < bytes.len() {
        *stopped = Some(format!(
            "process output exceeded the {} MiB capture limit",
            limit >> 20
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn final_buffered_stdin_write_is_flushed_and_its_error_is_observed() -> io::Result<()> {
        use std::pin::Pin;
        use std::task::{Context, Poll};
        struct Buffered {
            fail: bool,
            flushed: bool,
        }
        impl tokio::io::AsyncWrite for Buffered {
            fn poll_write(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
                bytes: &[u8],
            ) -> Poll<io::Result<usize>> {
                Poll::Ready(Ok(bytes.len()))
            }
            fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                if !self.flushed {
                    self.flushed = true;
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Poll::Ready(if self.fail {
                    Err(io::Error::other("deferred write failed"))
                } else {
                    Ok(())
                })
            }
            fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
                unreachable!()
            }
        }
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        runtime.block_on(async {
            for fail in [false, true] {
                let mut pipe = Buffered {
                    fail,
                    flushed: false,
                };
                let mut written = 0;
                assert!(!write_input(&mut pipe, b"input", &mut written).await?);
                assert_eq!(written, 5);
                assert!(!pipe.flushed, "acceptance is not completion");
                let result = write_input(&mut pipe, b"input", &mut written).await;
                assert!(pipe.flushed);
                if fail {
                    assert!(result.is_err_and(|e| e.to_string() == "deferred write failed"));
                } else {
                    assert!(result?);
                }
            }
            Ok(())
        })
    }

    /// A real child executable on every OS, independent of shell quoting/tools.
    #[test]
    fn child() -> Result<(), String> {
        let Ok(mode) = std::env::var("GITCRAFT_PROCESS_TEST_CHILD") else {
            return Ok(());
        };
        if let Ok(path) = std::env::var("GITCRAFT_PROCESS_TEST_PID") {
            std::fs::write(path, std::process::id().to_string()).map_err(|e| e.to_string())?;
        }
        match mode.as_str() {
            "flood" => {
                for _ in 0..128 {
                    std::io::stdout()
                        .write_all(&[b'o'; 8192])
                        .map_err(|e| e.to_string())?;
                    std::io::stderr()
                        .write_all(&[b'e'; 8192])
                        .map_err(|e| e.to_string())?;
                }
                let mut input = Vec::new();
                std::io::stdin()
                    .read_to_end(&mut input)
                    .map_err(|e| e.to_string())?;
                println!("INPUT={}", input.len());
            }
            "fail" => {
                eprintln!("child failed deliberately");
                std::process::exit(7);
            }
            "stdout-large" => {
                for _ in 0..128 {
                    std::io::stdout()
                        .write_all(&[b'x'; 8192])
                        .map_err(|e| e.to_string())?;
                }
            }
            "hang" => {
                println!("partial stdout");
                eprintln!("partial stderr");
                std::thread::sleep(Duration::from_secs(60));
            }
            "descendant" => {
                let marker =
                    std::env::var_os("GITCRAFT_PROCESS_TEST_EFFECT").ok_or("missing marker")?;
                let marker = std::path::PathBuf::from(marker);
                let child = command("side-effect")?
                    .stdin(Stdio::null())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                println!("DESCENDANT={}", child.id());
                until(|| marker.with_extension("ready").exists())?;
                std::process::exit(0);
            }
            "side-effect" => {
                let marker = std::path::PathBuf::from(
                    std::env::var_os("GITCRAFT_PROCESS_TEST_EFFECT").ok_or("missing marker")?,
                );
                std::fs::write(marker.with_extension("ready"), b"ready")
                    .map_err(|e| e.to_string())?;
                std::thread::sleep(Duration::from_secs(1));
                std::fs::write(marker, b"survived").map_err(|e| e.to_string())?;
            }
            "ready" => {
                print!("login banner\nREADY_");
                std::io::stdout().flush().map_err(|e| e.to_string())?;
                std::thread::sleep(Duration::from_millis(10));
                print!("MARKER\r\ncommand output");
            }
            "short" => std::thread::sleep(Duration::from_millis(150)),
            _ => return Err(format!("unknown child mode: {mode}")),
        }
        Ok(())
    }

    fn command(mode: &str) -> Result<Command, String> {
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .args(["processes::tests::child", "--exact", "--nocapture"])
            .env("GITCRAFT_PROCESS_TEST_CHILD", mode);
        Ok(command)
    }

    fn until(mut condition: impl FnMut() -> bool) -> Result<(), String> {
        let start = Instant::now();
        while !condition() {
            if start.elapsed() > Duration::from_secs(10) {
                return Err("condition timed out".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    #[test]
    fn drains_both_full_pipes_while_writing_large_input() -> Result<(), String> {
        let service = Service::new(2, 4).map_err(|e| e.to_string())?;
        let out = service
            .submit(
                command("flood")?,
                Options {
                    input: Some(vec![b'i'; 2 << 20]),
                    timeout: Some(Duration::from_secs(15)),
                    ..Default::default()
                },
            )?
            .wait()?
            .complete()?;
        assert!(out.status.success());
        assert!(out.stdout.iter().filter(|&&b| b == b'o').count() >= 1 << 20);
        assert_eq!(out.stderr, vec![b'e'; 1 << 20]);
        assert!(String::from_utf8_lossy(&out.stdout).contains("INPUT=2097152"));
        Ok(())
    }

    #[test]
    fn failures_and_spawn_errors_reach_the_caller() -> Result<(), String> {
        let service = Service::new(1, 4).map_err(|e| e.to_string())?;
        let out = service
            .submit(command("fail")?, Options::default())?
            .wait()?
            .complete()?;
        assert_eq!(out.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&out.stderr).contains("child failed deliberately"));
        let not_ready = service
            .submit(
                command("fail")?,
                Options {
                    stdout_ready: Some("never ready"),
                    ..Default::default()
                },
            )?
            .wait()?;
        assert!(
            not_ready
                .stopped
                .as_deref()
                .is_some_and(|s| s.contains("child failed deliberately"))
        );
        assert!(
            service
                .submit(
                    Command::new("gitcraft-no-such-executable-160"),
                    Options::default()
                )?
                .wait()
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn many_processes_share_bounded_slots_and_keep_their_results() -> Result<(), String> {
        let service = Service::new(3, 32).map_err(|e| e.to_string())?;
        let mut tickets = Vec::new();
        for _ in 0..16 {
            tickets.push(service.submit(command("short")?, Options::default())?);
        }
        for ticket in tickets {
            assert!(ticket.wait()?.complete()?.status.success());
        }
        assert_eq!(service.counts().spawned, 16);
        assert_eq!(service.counts().peak, 3);
        assert_eq!(service.counts().active, 0);
        Ok(())
    }

    #[test]
    fn queue_backpressure_is_immediate_and_cancelled_queued_work_never_spawns() -> Result<(), String>
    {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let first = service.submit(command("hang")?, Options::default())?;
        until(|| service.counts().spawned == 1)?;
        let second = service.submit(command("fail")?, Options::default())?;
        let third = service.submit(command("fail")?, Options::default())?;
        let start = Instant::now();
        assert!(
            service
                .submit(command("fail")?, Options::default())
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        second.cancel();
        third.cancel();
        first.cancel();
        assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
        assert!(second.wait().is_err());
        assert!(third.wait().is_err());
        assert_eq!(service.counts().spawned, 1);
        Ok(())
    }

    #[test]
    fn timeout_keeps_partial_output_and_reaps_the_child() -> Result<(), String> {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let started = Instant::now();
        let out = service
            .submit(
                command("hang")?,
                Options {
                    timeout: Some(Duration::from_millis(300)),
                    ..Default::default()
                },
            )?
            .wait()?;
        assert!(
            out.stopped
                .as_deref()
                .is_some_and(|s| s.contains("timed out"))
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("partial stdout"));
        assert!(String::from_utf8_lossy(&out.stderr).contains("partial stderr"));
        assert!(!out.status.success());
        assert!(started.elapsed() < Duration::from_secs(10));
        #[cfg(windows)]
        assert_eq!(out.status.code(), Some(1));
        Ok(())
    }

    #[test]
    fn output_overflow_is_bounded_and_reported() -> Result<(), String> {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let out = service
            .submit(
                command("flood")?,
                Options {
                    output_limit: 32768,
                    ..Default::default()
                },
            )?
            .wait()?;
        assert!(
            out.stopped
                .as_deref()
                .is_some_and(|s| s.contains("capture limit"))
        );
        assert!(out.stdout.len() <= 32768 && out.stderr.len() <= 32768);
        Ok(())
    }

    #[test]
    fn smaller_stderr_limit_stops_a_process_with_large_stdout_budget() -> Result<(), String> {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let out = service
            .submit(
                command("flood")?,
                Options {
                    output_limit: 2 << 20,
                    stderr_limit: Some(1024),
                    ..Default::default()
                },
            )?
            .wait()?;
        assert!(
            out.stopped
                .as_deref()
                .is_some_and(|s| s.contains("capture limit"))
        );
        assert!(out.stderr.len() <= 1024);
        assert!(out.stdout.len() < 2 << 20);
        Ok(())
    }

    #[test]
    fn exports_can_stream_to_disk_beyond_the_capture_limit() -> Result<(), String> {
        let path =
            std::env::temp_dir().join(format!("gitcraft-stream-export-{}", std::process::id()));
        let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let service = Service::new(1, 4).map_err(|e| e.to_string())?;
        let out = service
            .submit(
                command("stdout-large")?,
                Options {
                    stdout_file: Some(file),
                    output_limit: 1024,
                    ..Default::default()
                },
            )?
            .wait()?
            .complete()?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        let _ = std::fs::remove_file(path);
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
        assert!(size >= 1024 * 1024);
        Ok(())
    }

    #[test]
    fn dropping_a_ticket_cancels_it_and_shutdown_cleans_up_running_and_queued_children()
    -> Result<(), String> {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let first = service.submit(command("hang")?, Options::default())?;
        until(|| service.counts().spawned == 1)?;
        drop(first);
        until(|| service.counts().active == 0)?;
        let second = service.submit(command("hang")?, Options::default())?;
        until(|| service.counts().spawned == 2)?;
        let queued = service.submit(command("fail")?, Options::default())?;
        service.shutdown();
        assert_eq!(second.wait()?.stopped.as_deref(), Some("cancelled"));
        assert!(queued.wait().is_err());
        assert_eq!(service.counts().spawned, 2);
        assert_eq!(service.counts().active, 0);
        assert!(
            service
                .submit(command("fail")?, Options::default())
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn inherited_open_pipes_have_a_deadline_and_descendants_are_stopped() -> Result<(), String> {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let effect =
            std::env::temp_dir().join(format!("gitcraft-descendant-effect-{}", std::process::id()));
        let _ = std::fs::remove_file(&effect);
        let _ = std::fs::remove_file(effect.with_extension("ready"));
        let mut command = command("descendant")?;
        command.env("GITCRAFT_PROCESS_TEST_EFFECT", &effect);
        let start = Instant::now();
        let out = service
            .submit(
                command,
                Options {
                    drain_for: Duration::from_millis(100),
                    ..Default::default()
                },
            )?
            .wait()?;
        assert!(
            out.stopped
                .as_deref()
                .is_some_and(|s| s.contains("kept its output open"))
        );
        assert!(start.elapsed() < Duration::from_secs(10));
        assert!(String::from_utf8_lossy(&out.stdout).contains("DESCENDANT="));
        assert!(effect.with_extension("ready").exists());
        std::thread::sleep(Duration::from_millis(1200));
        let survived = effect.exists();
        let _ = std::fs::remove_file(&effect);
        let _ = std::fs::remove_file(effect.with_extension("ready"));
        assert!(!survived, "a descendant survived process-tree cleanup");
        Ok(())
    }

    #[test]
    fn timeouts_expire_while_queued_without_spawning() -> Result<(), String> {
        let service = Service::new(1, 4).map_err(|e| e.to_string())?;
        let first = service.submit(command("hang")?, Options::default())?;
        until(|| service.counts().spawned == 1)?;
        let timed = service.submit(
            command("short")?,
            Options {
                timeout: Some(Duration::from_millis(100)),
                ..Default::default()
            },
        )?;
        until(|| service.counts().queued == 0)?;
        assert!(timed.wait().is_err_and(|e| e.contains("timed out")));
        assert_eq!(service.counts().spawned, 1);
        first.cancel();
        assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
        Ok(())
    }

    #[test]
    fn idle_pipes_do_not_starve_other_child_io() -> Result<(), String> {
        let service = Service::new(3, 8).map_err(|e| e.to_string())?;
        let first = service.submit(command("hang")?, Options::default())?;
        let second = service.submit(command("hang")?, Options::default())?;
        until(|| service.counts().spawned == 2)?;
        let out = service
            .submit(
                command("short")?,
                Options {
                    timeout: Some(Duration::from_secs(3)),
                    ..Default::default()
                },
            )?
            .wait()?
            .complete()?;
        assert!(out.status.success());
        first.cancel();
        second.cancel();
        assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
        assert_eq!(second.wait()?.stopped.as_deref(), Some("cancelled"));
        Ok(())
    }
    #[test]
    fn remote_ready_marker_can_span_reads_and_keeps_the_first_output_bytes() -> Result<(), String> {
        let service = Service::new(1, 2).map_err(|e| e.to_string())?;
        let out = service
            .submit(
                command("ready")?,
                Options {
                    stdout_ready: Some("READY_MARKER"),
                    ..Default::default()
                },
            )?
            .wait()?
            .complete()?;
        assert!(out.stdout.starts_with(b"command output"));
        assert!(!String::from_utf8_lossy(&out.stdout).contains("login banner"));
        assert!(out.status.success());
        Ok(())
    }
    #[test]
    fn long_operations_leave_slots_for_interactive_reads() -> Result<(), String> {
        let service = Service::new(3, 8).map_err(|e| e.to_string())?;
        let first = service.submit(
            command("hang")?,
            Options {
                background: true,
                ..Default::default()
            },
        )?;
        until(|| service.counts().spawned == 1)?;
        let second = service.submit(
            command("hang")?,
            Options {
                background: true,
                ..Default::default()
            },
        )?;
        let read = service.submit(command("short")?, Options::default())?;
        assert!(read.wait()?.status.success());
        assert_eq!(
            service.counts().spawned,
            2,
            "a second long operation consumed a reserved read slot"
        );
        service.shutdown();
        assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
        assert!(second.wait().is_err());
        Ok(())
    }
}
