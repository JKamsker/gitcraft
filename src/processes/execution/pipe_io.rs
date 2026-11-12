//! Owned pipe handles and bounded capture; no process discovery or signalling.
use super::*;

pub(super) struct Pipes {
    pub input: Option<tokio::process::ChildStdin>,
    pub out: Option<tokio::process::ChildStdout>,
    pub err: Option<tokio::process::ChildStderr>,
    pub capture: Capture,
    pub written: usize,
    pub obuf: [u8; 8192],
    pub ebuf: [u8; 8192],
}

#[derive(Default)]
pub(super) struct Capture {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    preamble: Vec<u8>,
    pub ready: bool,
}

impl Pipes {
    pub fn open(child: &mut gitcraft_os::OwnedChild, ready: bool) -> Result<Self, String> {
        Ok(Self {
            input: child
                .stdin()
                .map(tokio::process::ChildStdin::from_std)
                .transpose()
                .map_err(|e| e.to_string())?,
            out: child
                .stdout()
                .map(tokio::process::ChildStdout::from_std)
                .transpose()
                .map_err(|e| e.to_string())?,
            err: child
                .stderr()
                .map(tokio::process::ChildStderr::from_std)
                .transpose()
                .map_err(|e| e.to_string())?,
            capture: Capture {
                ready,
                ..Capture::default()
            },
            written: 0,
            obuf: [0; 8192],
            ebuf: [0; 8192],
        })
    }

    pub fn stdout_read(
        &mut self,
        result: io::Result<usize>,
        options: &mut Options,
        blocking: &Arc<tokio::sync::Semaphore>,
        stopped: &mut Option<String>,
    ) {
        match result {
            Ok(0) => self.out = None,
            Ok(n) => self
                .capture
                .stdout(&self.obuf[..n], options, blocking, stopped),
            Err(e) => *stopped = Some(format!("could not read process output: {e}")),
        }
    }

    pub fn stderr_read(
        &mut self,
        result: io::Result<usize>,
        options: &mut Options,
        stopped: &mut Option<String>,
    ) {
        match result {
            Ok(0) => self.err = None,
            Ok(n) => self.capture.stderr(&self.ebuf[..n], options, stopped),
            Err(e) => *stopped = Some(format!("could not read process errors: {e}")),
        }
    }

    pub fn input_written(
        &mut self,
        result: io::Result<bool>,
        hold: bool,
        stopped: &mut Option<String>,
    ) {
        match result {
            Ok(true) => {
                if hold {
                    self.written = usize::MAX;
                } else {
                    self.input = None;
                }
            }
            Ok(false) => {}
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => self.input = None,
            Err(e) => *stopped = Some(format!("could not pass process its input: {e}")),
        }
    }

    /// Consume already buffered diagnostics, bounded even if inherited pipes stay open.
    pub async fn drain(
        &mut self,
        child: &mut gitcraft_os::OwnedChild,
        status: &mut Option<ExitStatus>,
        options: &mut Options,
    ) {
        let until = tokio::time::Instant::now()
            + if options.native_controller {
                Duration::from_secs(5)
            } else {
                options.drain_for
            };
        let mut overflow = None;
        while self.out.is_some()
            || self.err.is_some()
            || (options.native_controller && status.is_none())
        {
            if options.native_controller && status.is_none() {
                match child.observe() {
                    Ok(Some(s)) => *status = Some(s),
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            if self.out.is_none() && self.err.is_none() && status.is_some() {
                break;
            }
            tokio::select! {
                result = async { match &mut self.out { Some(p) => p.read(&mut self.obuf).await, None => std::future::pending().await } } => {
                    match result {
                        Ok(0) | Err(_) => self.out = None,
                        Ok(n) => if self.capture.ready {
                            self.capture.append_stdout(&self.obuf[..n], options, &mut overflow);
                        },
                    }
                }
                result = async { match &mut self.err { Some(p) => p.read(&mut self.ebuf).await, None => std::future::pending().await } } => {
                    match result {
                        Ok(0) | Err(_) => self.err = None,
                        Ok(n) => self.capture.stderr(&self.ebuf[..n], options, &mut overflow),
                    }
                }
                _ = tokio::time::sleep_until(until) => break,
                _ = tokio::time::sleep(POLL), if options.native_controller && status.is_none() => {},
            }
        }
    }
}

impl Capture {
    fn append_stdout(&mut self, bytes: &[u8], options: &mut Options, stopped: &mut Option<String>) {
        consume(&mut options.stdout, bytes, stopped);
        append(&mut self.stdout, bytes, options.output_limit, stopped);
    }

    pub fn stdout(
        &mut self,
        bytes: &[u8],
        options: &mut Options,
        blocking: &Arc<tokio::sync::Semaphore>,
        stopped: &mut Option<String>,
    ) {
        if self.ready {
            self.append_stdout(bytes, options, stopped);
            return;
        }
        for (i, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                if self.preamble.trim_ascii() == options.stdout_ready.unwrap_or_default().as_bytes()
                {
                    self.ready = true;
                    if let Some(connected) = options.ready.take() {
                        // Saving a credential can block; keep it off the I/O loop.
                        let blocking = Arc::clone(blocking);
                        tokio::spawn(async move {
                            if let Ok(_permit) = blocking.acquire_owned().await {
                                let _ = tokio::task::spawn_blocking(connected).await;
                            }
                        });
                    }
                    self.append_stdout(&bytes[i + 1..], options, stopped);
                    break;
                }
                self.preamble.clear();
            } else if self.preamble.len() < 65536 {
                self.preamble.push(byte);
            }
        }
    }

    pub fn stderr(&mut self, bytes: &[u8], options: &mut Options, stopped: &mut Option<String>) {
        let filtered = options.stderr_filter.as_mut().map(|filter| filter(bytes));
        let bytes = filtered.as_deref().unwrap_or(bytes);
        if let Some(consume) = &mut options.stderr
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| consume(bytes))).is_err()
        {
            *stopped = Some("process progress callback panicked".into());
        }
        append(
            &mut self.stderr,
            bytes,
            options.stderr_limit.unwrap_or(options.output_limit),
            stopped,
        );
    }
}

#[cfg(test)]
#[path = "pipe_io_tests.rs"]
mod tests;
