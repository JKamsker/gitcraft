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

const ACTIVE: usize = 8;
const QUEUED: usize = 256;
const OUTPUT_LIMIT: usize = 64 << 20;
const POLL: Duration = Duration::from_millis(20);

pub(crate) type Chunk = Box<dyn FnMut(&[u8]) + Send>;
pub(crate) struct Options {
    pub input: Option<Vec<u8>>,
    pub background: bool,
    pub timeout: Option<Duration>,
    pub cancel: Cancellation,
    pub cancelled: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub stderr: Option<Chunk>,
    pub ready: Option<Box<dyn FnOnce() + Send>>,
    /// Stream stdout directly to a file instead of capturing it in memory.
    pub stdout_file: Option<std::fs::File>,
    /// Cleanup on the other machine, before returning a cancelled/timed-out operation.
    /// This is a blocking protocol API, isolated from the I/O loop on bounded workers.
    pub stop_remote: Option<Box<dyn FnOnce() + Send>>,
    pub stop_ready: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub stdout_ready: Option<&'static str>,
    pub output_limit: usize,
    pub drain_for: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            input: None,
            background: false,
            timeout: None,
            cancel: crate::tasks::cancellation(),
            cancelled: None,
            stderr: None,
            ready: None,
            stdout_file: None,
            stop_remote: None,
            stop_ready: None,
            stdout_ready: None,
            output_limit: OUTPUT_LIMIT,
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
}

pub(crate) struct Ticket {
    reply: mpsc::Receiver<ResultOutput>,
    cancel: Cancellation,
}
impl Ticket {
    pub(crate) fn wait(self) -> ResultOutput {
        self.reply
            .recv()
            .map_err(|_| "Process service stopped before returning a result".to_owned())?
    }
    #[cfg(test)]
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
            })
            .is_err()
        {
            self.counters.queued.fetch_sub(1, Ordering::Relaxed);
            return Err("Process queue is full; try again when current work finishes".into());
        }
        Ok(Ticket {
            reply: receive,
            cancel: ticket,
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

async fn execute(
    command: Command,
    mut options: Options,
    deadline: Option<Instant>,
    ticket: &Cancellation,
    shutdown: &Cancellation,
    stats: &Counters,
    blocking: &Arc<tokio::sync::Semaphore>,
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
    let mut cmd = tokio::process::Command::from(command);
    cmd.stdin(if options.input.is_some() {
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
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    // Ordinary hooks/helpers inherit this group. Deliberate setsid escapes require stronger
    // OS containment; see docs/process-orchestration.md.
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run process: {e}"))?;
    #[cfg(windows)]
    let mut process_job = match child
        .id()
        .ok_or_else(|| "child has no process ID".to_owned())
        .and_then(gitcraft_os::supervise_suspended_process)
    {
        Ok(job) => Some(job),
        Err(error) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(format!("could not supervise process tree: {error}"));
        }
    };
    stats.spawned.fetch_add(1, Ordering::Relaxed);
    #[cfg(unix)]
    let pid = child.id();
    let mut input = child.stdin.take();
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
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
        if status.is_some() && out.is_none() && err.is_none() {
            break;
        }
        tokio::select! {
            result = async { match &mut out { Some(p) => p.read(&mut obuf).await, None => std::future::pending().await } } => {
                match result {
                    Ok(0) => out = None,
                    Ok(n) => {
                        if ready { append(&mut stdout, &obuf[..n], options.output_limit, &mut stopped); }
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
                        if let Some(consume) = &mut options.stderr
                            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| consume(&ebuf[..n]))).is_err() {
                            stopped = Some("process progress callback panicked".into());
                        }
                        append(&mut stderr, &ebuf[..n], options.output_limit, &mut stopped);
                    }
                    Err(e) => stopped = Some(format!("could not read process errors: {e}")),
                }
            }
            result = async {
                match &mut input {
                    Some(pipe) => pipe.write(&options.input.as_deref().unwrap_or_default()[written..]).await,
                    None => std::future::pending().await,
                }
            } => {
                match result {
                    Ok(0) => input = None,
                    Ok(n) => {
                        written += n;
                        if written == options.input.as_ref().map_or(0, Vec::len) { input = None; }
                    }
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => input = None,
                    Err(e) => stopped = Some(format!("could not pass process its input: {e}")),
                }
            }
            result = child.wait(), if status.is_none() => {
                match result {
                    Ok(s) => { status = Some(s); input = None; drain_deadline = Some(Instant::now() + options.drain_for); }
                    Err(e) => stopped = Some(format!("could not wait for process: {e}")),
                }
            }
            _ = tick.tick() => {
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
    if stopped.is_some() {
        // Remote cleanup may need its PID marker. The stderr callback has consumed every
        // chunk read so far. Run the legacy protocol off the event loop, before killing SSH.
        if let Some(stop) = options.stop_remote.take()
            && let Ok(_permit) = Arc::clone(blocking).acquire_owned().await
        {
            let _ = tokio::task::spawn_blocking(stop).await;
        }
        // The remote stop can also end the local loopback/SSH child. Reap it before
        // signaling its group: Darwin rejects SIGKILL on a group containing only zombies.
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|e| format!("could not reap process: {e}"))?;
        }
        #[cfg(unix)]
        if let Some(pid) = pid
            && let Err(error) = gitcraft_os::kill_process_group(pid)
        {
            stopped = Some(format!(
                "{}; could not stop process group: {error}",
                stopped.as_deref().unwrap_or_default()
            ));
        }
        #[cfg(windows)]
        drop(process_job.take());
        let _ = child.start_kill();
        if status.is_none() {
            status = Some(
                child
                    .wait()
                    .await
                    .map_err(|e| format!("could not reap process: {e}"))?,
            );
        }
        // Killing/reaping does not consume bytes already buffered in the pipes. Retain
        // those diagnostics without waiting indefinitely for an inherited pipe to close.
        let until = tokio::time::Instant::now() + options.drain_for;
        let mut overflow = None;
        while out.is_some() || err.is_some() {
            tokio::select! {
                result = async { match &mut out { Some(p) => p.read(&mut obuf).await, None => std::future::pending().await } } => {
                    match result {
                        Ok(0) | Err(_) => out = None,
                        Ok(n) => if ready { append(&mut stdout, &obuf[..n], options.output_limit, &mut overflow); },
                    }
                }
                result = async { match &mut err { Some(p) => p.read(&mut ebuf).await, None => std::future::pending().await } } => {
                    match result {
                        Ok(0) | Err(_) => err = None,
                        Ok(n) => {
                            if let Some(consume) = &mut options.stderr {
                                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| consume(&ebuf[..n])));
                            }
                            append(&mut stderr, &ebuf[..n], options.output_limit, &mut overflow);
                        }
                    }
                }
                _ = tokio::time::sleep_until(until) => break,
            }
        }
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
        stopped,
    })
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
