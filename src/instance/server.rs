//! The running instance's side: a listener on 127.0.0.1 that takes the requests of later starts
//! (see [`super::message`]) and keeps them for the window, which takes them each frame.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::discovery::{self, Found};
use super::lock::Held;
use super::message::{self, Refusal, Reply, Request};

/// How long a start has to send its request once connected.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Connections served at once; more are closed unread. A start sends one request and is done:
/// more than this at once is no gitcraft's.
const MAX_CONNECTIONS: usize = 8;

#[derive(Default)]
struct State {
    /// What later starts asked, the oldest first, until the window takes it.
    requests: Vec<Request>,
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
            if let Err(e) = discovery::write(dir, &found) {
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
        std::mem::take(&mut lock(&self.shared.state).requests)
    }

    /// The window is closing: later requests are answered "closing" (the start that sent one
    /// waits for the lock, see [`Self::shut_down`]).
    pub fn closing(&self) {
        lock(&self.shared.state).closing = true;
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
            for stream in listener.incoming().flatten() {
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

/// One request line, its `\n` included; `None` for one longer than [`message::MAX_LINE`], cut
/// short, or not text.
pub(super) fn read_line(stream: &TcpStream) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    BufReader::new(stream.take(message::MAX_LINE)).read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') {
        return Ok(None);
    }
    Ok(String::from_utf8(line).ok())
}

fn serve(shared: &Shared, mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(READ_TIMEOUT))?;
    let Some(line) = read_line(&stream)? else {
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

/// Keeps `request` for the window, unless it is closing.
fn take(shared: &Shared, request: Request) -> Reply {
    let repaint = {
        let mut state = lock(&shared.state);
        if state.closing {
            return Reply::Closing;
        }
        state.requests.push(request);
        state.repaint.clone()
    };
    if let Some(ctx) = repaint {
        ctx.request_repaint();
    }
    Reply::Ok
}
