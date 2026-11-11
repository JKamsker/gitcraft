//! Owned-child I/O pumping, cancellation settlement, and bounded output capture.
use super::*;

// The eighth argument only exists in opt-in libtest diagnostics.
#[cfg_attr(test, allow(clippy::too_many_arguments))]
pub(super) async fn execute(
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
    if let Some(reason) = before_start(cancelled(), deadline.is_some_and(|at| Instant::now() >= at))
    {
        return Err(reason.into());
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
    let began = Instant::now();
    let mut child = if options.native_controller {
        gitcraft_os::OwnedChild::spawn_controller(command)
    } else {
        gitcraft_os::OwnedChild::spawn(command)
    }
    .map_err(|e| format!("could not run process: {e}"))?;
    let spawned = Instant::now();
    let mut exited: Option<Instant> = None;
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
                        Ok(Some(s)) => { status = Some(s); exited = Some(Instant::now()); input = None; drain_deadline = Some(Instant::now() + options.drain_for); }
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
        if let Some(stop) = options.stop_remote.take() {
            let result = match Arc::clone(blocking).acquire_owned().await {
                Ok(_permit) => match tokio::task::spawn_blocking(stop).await {
                    Ok(result) => result,
                    Err(error) => Err(format!("cleanup worker failed: {error}")),
                },
                Err(error) => Err(format!("cleanup worker capacity unavailable: {error}")),
            };
            record_stop_confirmation(&mut stopped, &mut remote_confirmed, result);
        }
        // finish signals while the Unix leader is unreaped, then reaps exactly once;
        // Windows holds its Job through the final signal. No delayed numeric kills.
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Reaping);
        }
        if !options.native_controller {
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
        }
        // Killing/reaping does not consume bytes already buffered in the pipes. Retain
        // those diagnostics without waiting indefinitely for an inherited pipe to close.
        // A native helper may still be delivering its terminal output. Keep consuming
        // while it exits on EOF; never mark the sole owner expendable or kill it.
        let until = tokio::time::Instant::now()
            + if options.native_controller {
                Duration::from_secs(5)
            } else {
                options.drain_for
            };
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Draining);
        }
        let mut overflow = None;
        while out.is_some() || err.is_some() || (options.native_controller && status.is_none()) {
            if options.native_controller && status.is_none() {
                match child.observe() {
                    Ok(Some(s)) => status = Some(s),
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            if out.is_none() && err.is_none() && status.is_some() {
                break;
            }
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
                _ = tokio::time::sleep(POLL), if options.native_controller && status.is_none() => {},
            }
        }
    }
    // Also retire successful owners: descendants with closed output remain in the group.
    // This is idempotent when cancellation already finished it above.
    child.finish().map_err(|e| {
        format!(
            "{}; could not finish owned process: {e}",
            stopped.as_deref().unwrap_or("process completion failed")
        )
    })?;
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
    let ended = Instant::now();
    let exited = exited.unwrap_or(ended);
    Ok(Output {
        status: status.ok_or("process ended without an exit status")?,
        stdout,
        stderr,
        stop_kind: stop_kind(stopped.as_deref(), remote_confirmed),
        phases: Phases {
            queued: Duration::ZERO,
            spawn: spawned - began,
            ran: exited.saturating_duration_since(spawned),
            drained: ended.saturating_duration_since(exited),
        },
        stopped,
    })
}

/// Windows pipe writes can report acceptance before their blocking I/O finishes. Keep
/// polling that final write through flush before dropping the handle to deliver EOF,
/// while the child is alive. Child exit or BrokenPipe can still end input early.
/// Each await stays inside the main select so cancellation and deadlines still advance.
pub(super) async fn write_input<W: tokio::io::AsyncWrite + Unpin>(
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

pub(super) fn consume(callback: &mut Option<Chunk>, bytes: &[u8], stopped: &mut Option<String>) {
    if let Some(callback) = callback
        && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(bytes))).is_err()
    {
        *stopped = Some("process output callback panicked".into());
    }
}

pub(super) fn append(
    output: &mut Vec<u8>,
    bytes: &[u8],
    limit: usize,
    stopped: &mut Option<String>,
) {
    let keep = bytes.len().min(limit.saturating_sub(output.len()));
    output.extend_from_slice(&bytes[..keep]);
    if keep < bytes.len() {
        *stopped = Some(format!(
            "process output exceeded the {} MiB capture limit",
            limit >> 20
        ));
    }
}
