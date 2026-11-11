//! ssh's questions, asked in the window: a password, a key's passphrase, whether to trust a
//! host's key seen for the first time.
//!
//! ssh is started with this app as its `SSH_ASKPASS` program. Started so, the app doesn't open
//! a window (see [`run_helper`]): it hands the question to the running app over a connection
//! to 127.0.0.1, with a token only the ssh it started knows, and prints the answer for ssh.
//! The running app shows the question (see [`pending`] and [`answer`]) and sends the answer
//! back. A password or passphrase typed is remembered for the machine while the app runs, so
//! the other shells kept open there don't ask again; one ssh asks again, and it was wrong: it
//! is forgotten, and the question shown again. Typed with "Remember connection", it is also
//! kept in the system's credential store (see [`super::vault`]), but only once the ssh given
//! it has signed in (see [`Attempt`]), and given from there after a restart, until it is wrong
//! or forgotten.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine;

use super::shell::lock;

/// Where the helper finds the running app: `<port> <token> <session>`.
const ENV: &str = "GITCRAFT_ASKPASS";

/// What a question asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Something secret typed (a password, a passphrase).
    Secret,
    /// Whether to trust a host's key: answered `yes` or `no`.
    TrustHost,
    /// Whether to go on (ssh's `SSH_ASKPASS_PROMPT=confirm`).
    Confirm,
    /// Something to do elsewhere (touching a security key), nothing to answer.
    Notice,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Secret => "secret",
            Kind::TrustHost => "trust",
            Kind::Confirm => "confirm",
            Kind::Notice => "notice",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "secret" => Kind::Secret,
            "trust" => Kind::TrustHost,
            "confirm" => Kind::Confirm,
            "notice" => Kind::Notice,
            _ => return None,
        })
    }

    /// What ssh asks with `prompt` (and `SSH_ASKPASS_PROMPT` set to `how`).
    fn of(prompt: &str, how: Option<&str>) -> Self {
        match how {
            Some("confirm") => Kind::Confirm,
            Some("none") => Kind::Notice,
            _ if prompt.contains("(yes/no") => Kind::TrustHost,
            _ => Kind::Secret,
        }
    }
}

/// A question of ssh's waiting for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub id: u64,
    /// The machine being connected to ("SSH · build").
    pub host: String,
    pub prompt: String,
    pub kind: Kind,
    /// A secret kept for it (see [`super::vault`]) was wrong, and is forgotten.
    pub forgotten: bool,
}

/// An answer: what to give ssh, and whether to keep it for good.
type Reply = Option<(String, bool)>;

struct Waiting {
    question: Question,
    reply: Sender<Reply>,
}

#[derive(Default)]
struct State {
    waiting: Vec<Waiting>,
    /// The machine each ssh started is connecting to, by its session number.
    sessions: HashMap<u64, String>,
    /// Secrets typed, by machine and question.
    known: HashMap<(String, String), String>,
    /// The questions a session was answered from `known`: asked again, that answer was wrong.
    answered_from_known: HashSet<(u64, String)>,
    /// When a question was last shown or answered.
    last_asked: Option<Instant>,
    /// What the window is to say (a secret that couldn't be kept), the oldest first.
    notices: Vec<String>,
    /// Secrets typed with "Remember connection", by the session given them: kept in the
    /// system's store once that ssh has connected (see [`Attempt`]), dropped if it doesn't.
    to_keep: HashMap<u64, Vec<ToKeep>>,
    repaint: Option<egui::Context>,
}

/// A secret to keep once the connection it was given to is made.
struct ToKeep {
    host: String,
    prompt: String,
    secret: String,
}

/// One ssh started, until it is known whether it connected. What it was given to remember is
/// kept only once it has ([`Attempt::connected`]); dropped without, it is not.
pub(super) struct Attempt {
    session: u64,
}

impl Attempt {
    /// The ssh signed in (its shell answered): what it was given to remember is kept now, in
    /// the system's store (blocking: never on the window's thread).
    pub(super) fn connected(self) {
        let Some(server) = server() else { return };
        let kept = lock(&server.state)
            .to_keep
            .remove(&self.session)
            .unwrap_or_default();
        for k in kept {
            if let Err(e) = super::vault::keep(&k.host, &k.prompt, &k.secret) {
                tell(
                    server,
                    format!("{}: the password couldn't be saved ({e}).", k.host),
                );
            }
        }
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        // Not connected (or kept already): nothing of it is kept.
        if let Some(server) = server() {
            lock(&server.state).to_keep.remove(&self.session);
        }
    }
}

struct Server {
    port: u16,
    token: String,
    state: Mutex<State>,
}

fn server() -> Option<&'static Server> {
    static SERVER: OnceLock<Option<Server>> = OnceLock::new();
    SERVER
        .get_or_init(|| {
            let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
            let port = listener.local_addr().ok()?.port();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    std::thread::spawn(move || {
                        let _ = serve(stream);
                    });
                }
            });
            Some(Server {
                port,
                token: random_token(),
                state: Mutex::default(),
            })
        })
        .as_ref()
}

/// 128 random bits as hex, from the standard library's own randomly seeded hashers.
pub(crate) fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// Lets the window know when a question comes (it redraws to show it).
pub fn set_repaint(ctx: egui::Context) {
    if let Some(server) = server() {
        lock(&server.state).repaint = Some(ctx);
    }
}

/// Set in a process without a window (`gitcraft mcp`): nobody could answer ssh's questions, so
/// ssh asks none and fails at once, saying why, rather than wait for an answer forever.
static NO_WINDOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Says that this process has no window to ask ssh's questions in (see [`NO_WINDOW`]).
pub fn no_window() {
    NO_WINDOW.store(true, Ordering::Relaxed);
}

/// The program and environment an ssh connecting to `host` is started with, so that its
/// questions come to the window, and the attempt to tell once it has connected; `None` when
/// there's no way to (no listener could be made).
pub(super) fn env_for(host_label: &str) -> Option<(Vec<(&'static str, String)>, Attempt)> {
    static SESSIONS: AtomicU64 = AtomicU64::new(1);
    if NO_WINDOW.load(Ordering::Relaxed) {
        return None;
    }
    let server = server()?;
    let program = std::env::current_exe().ok()?;
    let session = SESSIONS.fetch_add(1, Ordering::Relaxed);
    lock(&server.state)
        .sessions
        .insert(session, host_label.to_owned());
    let env = vec![
        ("SSH_ASKPASS", program.display().to_string()),
        ("SSH_ASKPASS_REQUIRE", "force".to_owned()),
        (ENV, format!("{} {} {session}", server.port, server.token)),
    ];
    Some((env, Attempt { session }))
}

/// Whether ssh asked something lately (or waits for an answer): a connection being made then
/// waits for the user, not for the network, and is given the time.
pub(super) fn asked_lately(within: Duration) -> bool {
    let Some(server) = server() else {
        return false;
    };
    let state = lock(&server.state);
    !state.waiting.is_empty() || state.last_asked.is_some_and(|at| at.elapsed() < within)
}

/// The question to show, the oldest first.
pub fn pending() -> Option<Question> {
    let server = server()?;
    lock(&server.state)
        .waiting
        .first()
        .map(|w| w.question.clone())
}

/// Answers question `id`: `None` declines it.
pub fn answer(id: u64, answer: Option<String>) {
    reply(id, answer.map(|a| (a, false)));
}

/// Answers question `id` with `secret`, and keeps it in the system's credential store.
pub fn answer_and_remember(id: u64, secret: String) {
    reply(id, Some((secret, true)));
}

fn reply(id: u64, reply: Reply) {
    let Some(server) = server() else { return };
    let mut state = lock(&server.state);
    state.last_asked = Some(Instant::now());
    if let Some(i) = state.waiting.iter().position(|w| w.question.id == id) {
        let waiting = state.waiting.remove(i);
        let _ = waiting.reply.send(reply);
    }
}

/// What the window is to say since it last asked (a secret that couldn't be kept, the sign-ins
/// forgotten).
pub fn take_notices() -> Vec<String> {
    server()
        .map(|server| std::mem::take(&mut lock(&server.state).notices))
        .unwrap_or_default()
        .into_iter()
        .map(|notice| super::display::diagnostic(&notice))
        .collect()
}

static SECURITY_EPOCH: AtomicU64 = AtomicU64::new(0);
static HOST_EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

/// Invalidating a saved/rejected credential must not retain an authenticated transport.
#[cfg(unix)]
pub(super) fn security_epoch(host: &str) -> [u64; 2] {
    let host_epoch = HOST_EPOCHS
        .get()
        .and_then(|epochs| lock(epochs).get(host).copied())
        .unwrap_or(0);
    [SECURITY_EPOCH.load(Ordering::Relaxed), host_epoch]
}

fn invalidate_host(host: &str) {
    let mut epochs = lock(HOST_EPOCHS.get_or_init(Mutex::default));
    let epoch = epochs.entry(host.to_owned()).or_default();
    *epoch = epoch.wrapping_add(1);
}

/// Forgets every sign-in kept, here and in the system's store: blocking, so not on the
/// window's thread. The window is told how many there were.
pub fn forget_saved() {
    SECURITY_EPOCH.fetch_add(1, Ordering::Relaxed);
    let Some(server) = server() else { return };
    lock(&server.state).known.clear();
    let count = super::vault::forget_all();
    let notice = match count {
        0 => "No saved sign-ins to forget.".to_owned(),
        1 => "Forgot 1 saved sign-in.".to_owned(),
        n => format!("Forgot {n} saved sign-ins."),
    };
    tell(server, notice);
}

/// Has the window say `notice`.
fn tell(server: &Server, notice: String) {
    let repaint = {
        let mut state = lock(&server.state);
        state.notices.push(notice);
        state.repaint.clone()
    };
    if let Some(ctx) = repaint {
        ctx.request_repaint();
    }
}

/// One question from a helper: `<token> <session> <kind> <prompt in base64>`, answered
/// `OK <answer in base64>` or `NO`.
/// The longest request a helper sends: ssh's prompt (a few lines at most) in base64, and the
/// words before it. Anything can connect to the port: what is longer is no helper's, and is
/// refused before it is held, let alone its token checked.
const MAX_REQUEST: u64 = 64 * 1024;

/// Compare tokens without revealing how much of one matched.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Asks the app `found` names (`<port> <token> <session>`) `prompt`, of `kind`: its answer,
/// `None` when declined.
fn ask_app(found: &str, prompt: &str, kind: Kind) -> std::io::Result<Option<String>> {
    let mut parts = found.split_whitespace();
    let (Some(port), Some(token), Some(session)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(std::io::Error::other("no app to ask"));
    };
    let port: u16 = port.parse().map_err(std::io::Error::other)?;
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(prompt);
    writeln!(stream, "{token} {session} {} {encoded}", kind.word())?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    Ok(reply.trim().strip_prefix("OK").map(|rest| {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(rest.trim())
            .unwrap_or_default();
        String::from_utf8_lossy(&decoded).into_owned()
    }))
}

/// When this process was started as ssh's askpass program (see the module's doc): asks the
/// running app and returns the exit code to end with; `None` for an ordinary start.
pub fn run_helper() -> Option<i32> {
    let found = std::env::var(ENV).ok()?;
    let mut args = std::env::args().skip(1);
    let prompt = args.next()?;
    if args.next().is_some() {
        return None;
    }
    let how = std::env::var("SSH_ASKPASS_PROMPT").ok();
    let kind = Kind::of(&prompt, how.as_deref());
    match (kind, ask_app(&found, &prompt, kind)) {
        // Nothing to answer: ssh ends this program once the key was touched.
        (Kind::Notice, _) => {
            std::thread::sleep(Duration::from_secs(600));
            Some(0)
        }
        (Kind::Confirm, Ok(Some(_))) => Some(0),
        (_, Ok(Some(answer))) => {
            println!("{answer}");
            Some(0)
        }
        _ => Some(1),
    }
}

/// Held by the tests that put questions in the window's queue (one queue for the process),
/// and by the tests of a window that would show them: a question from one test closes the
/// other's command palette.
#[cfg(test)]
pub(crate) fn asking() -> std::sync::MutexGuard<'static, ()> {
    static ASKING: Mutex<()> = Mutex::new(());
    ASKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests;

mod broker;
use broker::*;
