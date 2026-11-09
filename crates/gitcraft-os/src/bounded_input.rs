//! A bounded, retained writer for a transport's actual stdin pipe. No thread/PID lookup.
use std::io::{self, Write};
use std::process::ChildStdin;
use std::sync::{
    Arc, Mutex, OnceLock, PoisonError,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
const LIMIT: usize = 64;
const TICK: Duration = Duration::from_millis(5);
static COUNT: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Permit {
    fn acquire() -> io::Result<Self> {
        COUNT
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < LIMIT).then_some(n + 1)
            })
            .map(|_| Self)
            .map_err(|_| io::Error::other("transport writer capacity exhausted"))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Retained {
    thread: JoinHandle<()>,
    _permit: Permit,
}
fn interrupt(thread: &JoinHandle<()>) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{Foundation::HANDLE, System::IO::CancelSynchronousIo};
        // SAFETY: this retained JoinHandle owns precisely the dedicated pipe writer thread.
        // No unrelated work is ever scheduled on it; repeated cancellation covers check/I/O races.
        let _ = unsafe { CancelSynchronousIo(HANDLE(thread.as_raw_handle())) };
    }
    #[cfg(not(windows))]
    let _ = thread;
}
static RETAINED: Mutex<Vec<Retained>> = Mutex::new(Vec::new());
fn reap_retained() {
    let mut pending = RETAINED.lock().unwrap_or_else(PoisonError::into_inner);
    let mut i = 0;
    while i < pending.len() {
        if pending[i].thread.is_finished() {
            let owner = pending.swap_remove(i);
            let _ = owner.thread.join();
        } else {
            interrupt(&pending[i].thread);
            i += 1;
        }
    }
}
fn registry() -> io::Result<()> {
    static REGISTRY: OnceLock<Result<JoinHandle<()>, String>> = OnceLock::new();
    reap_retained();
    match REGISTRY.get_or_init(|| {
        thread::Builder::new()
            .name("transport-writer-retirement".into())
            .spawn(|| {
                loop {
                    reap_retained();
                    thread::sleep(TICK);
                }
            })
            .map_err(|e| e.to_string())
    }) {
        Ok(thread) if !thread.is_finished() => Ok(()),
        _ => Err(io::Error::other(
            "transport writer retirement service unavailable",
        )),
    }
}

struct Request {
    bytes: Vec<u8>,
    reply: mpsc::Sender<io::Result<()>>,
}
pub struct BoundedInput {
    requests: Option<mpsc::SyncSender<Request>>,
    stop: Arc<AtomicBool>,
    owner: Option<Retained>,
}
impl BoundedInput {
    pub fn new(mut stdin: ChildStdin) -> io::Result<Self> {
        let permit = Permit::acquire()?;
        registry()?;
        nonblocking(&stdin)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (tx, rx) = mpsc::sync_channel::<Request>(1);
        let thread = thread::Builder::new()
            .name("transport-stdin".into())
            .spawn(move || {
                while let Ok(request) = rx.recv() {
                    let result = write_owned(
                        &mut stdin,
                        &request.bytes,
                        || !stopping.load(Ordering::Acquire),
                        || thread::sleep(TICK),
                    );
                    let failed = result.is_err();
                    let _ = request.reply.send(result);
                    if failed || stopping.load(Ordering::Acquire) {
                        break;
                    }
                }
                // Closing the real pipe here delivers EOF; a blocked platform write is retained
                // and repeatedly interrupted through this thread's owned HANDLE until it exits.
            })?;
        Ok(Self {
            requests: Some(tx),
            stop,
            owner: Some(Retained {
                thread,
                _permit: permit,
            }),
        })
    }
    /// Once any byte could have been submitted, failure must be treated as ambiguous.
    /// `waiting` owns the caller's absolute deadline/cancellation policy, including flush.
    pub fn write_until(
        &mut self,
        bytes: Vec<u8>,
        mut waiting: impl FnMut() -> bool,
    ) -> io::Result<()> {
        if bytes.len() > 96 << 20 {
            return Err(io::Error::other("oversized transport input"));
        }
        if !waiting() {
            self.close();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "transport write cancelled or expired",
            ));
        }
        let (reply, rx) = mpsc::channel();
        let sent = self
            .requests
            .as_ref()
            .ok_or_else(|| io::Error::other("transport input closed"))?
            .try_send(Request { bytes, reply });
        if sent.is_err() {
            self.close();
            return Err(io::Error::other("transport writer unavailable"));
        }
        loop {
            if !waiting() {
                self.close();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "transport write cancelled or expired; delivery unknown",
                ));
            }
            match rx.recv_timeout(TICK) {
                Ok(Ok(())) if waiting() => return Ok(()),
                Ok(Ok(())) => {
                    self.close();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "transport write expired after delivery",
                    ));
                }
                Ok(Err(e)) => {
                    self.close();
                    return Err(e);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.close();
                    return Err(io::Error::other("transport writer lost"));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
    fn close(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.requests.take();
        if let Some(owner) = self.owner.take() {
            interrupt(&owner.thread);
            if owner.thread.is_finished() {
                let _ = owner.thread.join();
            } else {
                // A process-lifetime registry retains every unfinished JoinHandle and
                // its admission permit. Even a failed monitor cannot detach/drop ownership;
                // subsequent writer admission checks monitor health and fails closed.
                RETAINED
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(owner);
            }
        }
    }
}
impl Drop for BoundedInput {
    fn drop(&mut self) {
        self.close();
    }
}
#[cfg(unix)]
fn nonblocking(stdin: &ChildStdin) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: access only the held pipe's flags, before giving it to its dedicated writer.
    let flags = unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(windows)]
fn nonblocking(_: &ChildStdin) -> io::Result<()> {
    Ok(())
}
fn write_owned(
    writer: &mut impl Write,
    bytes: &[u8],
    mut waiting: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> io::Result<()> {
    let check = |waiting: &mut dyn FnMut() -> bool| {
        if waiting() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned write cancelled",
            ))
        }
    };
    let mut sent = 0;
    while sent < bytes.len() {
        check(&mut waiting)?;
        match writer.write(&bytes[sent..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => sent += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause()
            }
            Err(e) => return Err(e),
        }
    }
    loop {
        check(&mut waiting)?;
        match writer.flush() {
            Ok(()) => return check(&mut waiting),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause()
            }
            Err(e) => return Err(e),
        }
    }
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn partial_writes_repeated_backpressure_and_flush_obey_cancellation() {
        struct Sink<'a> {
            calls: &'a Cell<usize>,
            sent: usize,
            flush: bool,
        }
        impl Write for Sink<'_> {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.calls.set(self.calls.get() + 1);
                if self.sent == 0 {
                    self.sent += 1;
                    Ok(b.len().min(1))
                } else {
                    Err(io::ErrorKind::WouldBlock.into())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flush = true;
                self.calls.set(self.calls.get() + 1);
                Err(io::ErrorKind::WouldBlock.into())
            }
        }
        for data in [b"hello".as_slice(), b"h"] {
            let calls = Cell::new(0);
            let mut sink = Sink {
                calls: &calls,
                sent: 0,
                flush: false,
            };
            assert_eq!(
                write_owned(&mut sink, data, || calls.get() < 8, || {})
                    .err()
                    .map(|e| e.kind()),
                Some(io::ErrorKind::TimedOut)
            );
            assert_eq!(calls.get(), 8);
            assert_eq!(sink.sent, 1);
            assert_eq!(sink.flush, data.len() == 1);
        }
    }
    #[test]
    fn no_partial_or_flush_failure_can_become_success_or_replay() {
        struct Sink {
            left: usize,
            sent: usize,
            flush_error: bool,
        }
        impl Write for Sink {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                let n = self.left.min(b.len());
                if n == 0 {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                self.left -= n;
                self.sent += n;
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                if self.flush_error {
                    Err(io::ErrorKind::BrokenPipe.into())
                } else {
                    Ok(())
                }
            }
        }
        for (left, flush_error) in [(0, false), (4, false), (5, true)] {
            let mut sink = Sink {
                left,
                sent: 0,
                flush_error,
            };
            assert!(write_owned(&mut sink, b"hello", || true, || {}).is_err());
            assert_eq!(sink.sent, left);
        }
    }
}
