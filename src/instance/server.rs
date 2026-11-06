//! The running instance's side: a listener on 127.0.0.1 that takes the requests of later starts
//! (see [`super::message`]) and keeps them for the window, which takes them each frame.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::discovery::{self, Found};
use super::lock::Held;
use super::message::{self, Refusal, Reply, Request};

/// How long a start has to send its whole request once connected: one sending a byte now and
/// then would otherwise hold its connection for as long as it likes.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Connections served at once; more are closed unread. A start sends one request and is done:
/// more than this at once is no gitcraft's.
/// How long a request waits for the window to take it before it is answered all the same (it
/// stays queued: a window that is minimized may draw only now and then). Until then, a window
/// closing answers it "closing", and the start that sent it takes over.
#[cfg(not(test))]
const WINDOW_WAIT: Duration = Duration::from_secs(3);
#[cfg(test)]
const WINDOW_WAIT: Duration = Duration::from_millis(1500);

pub(super) const MAX_CONNECTIONS: usize = 8;

#[derive(Default)]
struct State {
    /// What later starts asked, the oldest first, until the window takes it, each with where
    /// to say that it was taken.
    requests: Vec<(Request, Sender<Reply>)>,
    /// The window closed: nothing more is taken.
    closing: bool,
    repaint: Option<egui::Context>,
}

/// What the listener's threads and the window share.
struct Shared {
    token: String,
    state: Mutex<State>,
    serving: AtomicUsize,
}

/// This process, as the running instance: it holds the lock, and listens for later starts.
pub struct Primary {
    dir: PathBuf,
    shared: Arc<Shared>,
    /// The port listened on; `None` when no listener could be made (later starts then find an
    /// instance that doesn't answer, and say so, rather than starting a second one).
    port: Option<u16>,
    lock: Mutex<Option<Held>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Primary {
    /// Starts listening, and writes where to `dir` for later starts. `held` is the lock that
    /// makes this process the running instance; it is kept until [`Self::shut_down`].
    pub fn start(dir: &Path, held: Held) -> Self {
        let shared = Arc::new(Shared {
            token: crate::remote::askpass::random_token(),
            state: Mutex::default(),
            serving: AtomicUsize::new(0),
        });
        // Holding the lock, any file there is a crashed instance's: gone before anything else,
        // so that no start trusts it (whoever has its port now) if this one can't write its own.
        discovery::remove(dir);
        let port = match listen(Arc::clone(&shared)) {
            Ok(port) => Some(port),
            Err(e) => {
                eprintln!("gitcraft: later starts can't reach this window: {e}");
                None
            }
        };
        if let Some(port) = port {
            let found = Found {
                v: message::VERSION,
                port,
                token: shared.token.clone(),
                pid: std::process::id(),
            };
            // Once more after a moment: a virus scanner may hold the file for an instant.
            let written = discovery::write(dir, &found).or_else(|_| {
                std::thread::sleep(Duration::from_millis(100));
                discovery::write(dir, &found)
            });
            if let Err(e) = written {
                eprintln!("gitcraft: later starts can't find this window: {e}");
            }
        }
        Self {
            dir: dir.to_owned(),
            shared,
            port,
            lock: Mutex::new(Some(held)),
        }
    }

    /// The port listened on (for tests).
    #[cfg(test)]
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// The token later starts are to send (for tests).
    #[cfg(test)]
    pub fn token(&self) -> &str {
        &self.shared.token
    }

    /// Has the window redrawn when a request comes.
    pub fn set_repaint(&self, ctx: egui::Context) {
        lock(&self.shared.state).repaint = Some(ctx);
    }

    /// What later starts asked since the last call, the oldest first.
    pub fn take_requests(&self) -> Vec<Request> {
        let taken = std::mem::take(&mut lock(&self.shared.state).requests);
        taken
            .into_iter()
            .map(|(request, taken)| {
                let _ = taken.send(Reply::Ok);
                request
            })
            .collect()
    }

    /// The window is closing: later requests are answered "closing" (the start that sent one
    /// waits for the lock, see [`Self::shut_down`]).
    pub fn closing(&self) {
        let left = {
            let mut state = lock(&self.shared.state);
            state.closing = true;
            std::mem::take(&mut state.requests)
        };
        // Not taken by the window: their starts wait for the lock instead.
        for (_, taken) in left {
            let _ = taken.send(Reply::Closing);
        }
    }

    /// How many requests wait for the window (for tests).
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        lock(&self.shared.state).requests.len()
    }

    /// The window closed and what it keeps is on disk: the file naming this instance goes, and
    /// the lock is let go, so that the next start becomes the running instance.
    pub fn shut_down(&self) {
        self.closing();
        if self.port.is_some() {
            discovery::remove_if_ours(&self.dir, &self.shared.token);
        }
        lock(&self.lock).take();
    }
}

/// Binds 127.0.0.1 on a port of the system's choosing and serves it in the background.
fn listen(shared: Arc<Shared>) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    std::thread::Builder::new()
        .name("gitcraft-instance".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                let stream = match stream {
                    Ok(stream) => stream,
                    // Out of file handles, say: not tried again at once, over and over.
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                };
                if shared.serving.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                    shared.serving.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                let spawned = std::thread::Builder::new()
                    .name("gitcraft-instance-request".to_owned())
                    .spawn({
                        let shared = Arc::clone(&shared);
                        move || {
                            let _ = serve(&shared, stream);
                            shared.serving.fetch_sub(1, Ordering::AcqRel);
                        }
                    });
                if spawned.is_err() {
                    shared.serving.fetch_sub(1, Ordering::AcqRel);
                }
            }
        })?;
    Ok(port)
}

/// One line, its `\n` included, read within `within` all told; `None` for one longer than
/// [`message::MAX_LINE`], cut short, too slow, or not text.
pub(super) fn read_line(
    mut stream: &TcpStream,
    within: Duration,
) -> std::io::Result<Option<String>> {
    let deadline = Instant::now() + within;
    let limit = usize::try_from(message::MAX_LINE).unwrap_or(usize::MAX);
    let mut line = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        stream.set_read_timeout(Some(left))?;
        let read = match stream.read(&mut chunk) {
            Ok(0) => return Ok(None),
            Ok(read) => read,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let got = &chunk[..read];
        let end = got.iter().position(|&b| b == b'\n');
        line.extend_from_slice(&got[..end.map_or(read, |i| i + 1)]);
        if line.len() > limit {
            return Ok(None);
        }
        if end.is_some() {
            return Ok(String::from_utf8(line).ok());
        }
    }
}

fn serve(shared: &Shared, mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_write_timeout(Some(READ_TIMEOUT))?;
    let Some(line) = read_line(&stream, READ_TIMEOUT)? else {
        return Ok(());
    };
    let reply = match message::decode_request(&line, &shared.token) {
        // Not a gitcraft's, or not this user's: nothing is told.
        Err(Refusal::Malformed | Refusal::Stranger) => return Ok(()),
        Err(Refusal::Unusable(why)) => Reply::Refused(why),
        Ok(request) => take(shared, request),
    };
    stream.write_all(message::encode_reply(&reply).as_bytes())
}

/// Keeps `request` for the window, unless it is closing, and waits a while for the window to
/// take it.
fn take(shared: &Shared, request: Request) -> Reply {
    let (tx, rx) = channel();
    let repaint = {
        let mut state = lock(&shared.state);
        if state.closing {
            return Reply::Closing;
        }
        state.requests.push((request, tx));
        state.repaint.clone()
    };
    if let Some(ctx) = repaint {
        ctx.request_repaint();
    }
    rx.recv_timeout(WINDOW_WAIT).unwrap_or(Reply::Ok)
}
