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
}

static SECURITY_EPOCH: AtomicU64 = AtomicU64::new(0);
static HOST_EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

/// Invalidating a saved/rejected credential must not retain an authenticated transport.
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

/// One request line, its `\n` included; `None` for one longer than [`MAX_REQUEST`], cut
/// short, or not text.
fn read_request(stream: &TcpStream) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    BufReader::new(stream.take(MAX_REQUEST)).read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') {
        return Ok(None);
    }
    Ok(String::from_utf8(line).ok())
}

fn serve(stream: TcpStream) -> std::io::Result<()> {
    static IDS: AtomicU64 = AtomicU64::new(1);
    let Some(server) = server() else {
        return Ok(());
    };
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let Some(line) = read_request(&stream)? else {
        return Ok(());
    };
    stream.set_read_timeout(None)?;
    let mut words = line.split_whitespace();
    let (Some(token), Some(session), Some(kind), prompt) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return Ok(());
    };
    // Only an ssh this app started knows the token.
    if !same(token.as_bytes(), server.token.as_bytes()) {
        return Ok(());
    }
    let (Ok(session), Some(kind)) = (session.parse::<u64>(), Kind::from_word(kind)) else {
        return Ok(());
    };
    let prompt = prompt
        .and_then(|p| base64::engine::general_purpose::STANDARD.decode(p).ok())
        .map(|p| String::from_utf8_lossy(&p).trim().to_owned())
        .unwrap_or_default();
    let reply = ask(
        server,
        session,
        kind,
        prompt,
        IDS.fetch_add(1, Ordering::Relaxed),
    );
    let mut stream = stream;
    match reply {
        Some(answer) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(answer);
            writeln!(stream, "OK {encoded}")
        }
        None => writeln!(stream, "NO"),
    }
}

/// What is known of a secret's answer before the user is asked.
enum Known {
    /// Give this.
    Use(String),
    /// What was given this ssh before was wrong (and is forgotten here).
    Wrong,
    /// Nothing here: maybe in the system's store.
    Nothing,
}

/// The answer to `prompt` of `session`'s ssh: a secret known for its machine (typed since the
/// app started, or kept in the system's store), else the user's.
fn ask(server: &Server, session: u64, kind: Kind, prompt: String, id: u64) -> Option<String> {
    let host = lock(&server.state)
        .sessions
        .get(&session)
        .cloned()
        .unwrap_or_else(|| "SSH".to_owned());
    let mut forgotten = false;
    if kind == Kind::Secret {
        match known(server, session, &host, &prompt) {
            Known::Use(secret) => return Some(secret),
            // Not to be kept, nor kept any longer if it was.
            Known::Wrong => {
                forget_to_keep(server, session, &prompt);
                forgotten = super::vault::forget(&host, &prompt);
            }
            Known::Nothing => {
                if let Some(saved) = super::vault::recall(&host, &prompt) {
                    remember(server, session, &host, &prompt, &saved);
                    return Some(saved);
                }
            }
        }
    }
    let (tx, rx) = channel();
    let repaint = {
        let mut state = lock(&server.state);
        state.waiting.push(Waiting {
            question: Question {
                id,
                host: host.clone(),
                prompt: prompt.clone(),
                kind,
                forgotten,
            },
            reply: tx,
        });
        state.last_asked = Some(Instant::now());
        state.repaint.clone()
    };
    if let Some(ctx) = repaint {
        ctx.request_repaint();
    }
    let (answer, keep) = rx.recv().ok().flatten()?;
    if kind == Kind::Secret {
        remember(server, session, &host, &prompt, &answer);
        // Kept only once this ssh has connected with it (see `Attempt`): a wrong one never is.
        if keep {
            let mut state = lock(&server.state);
            let list = state.to_keep.entry(session).or_default();
            list.retain(|k| k.prompt != prompt);
            list.push(ToKeep {
                host,
                prompt,
                secret: answer.clone(),
            });
        }
    }
    Some(answer)
}

/// `session`'s answer to `prompt` was wrong: not to be kept.
fn forget_to_keep(server: &Server, session: u64, prompt: &str) {
    if let Some(list) = lock(&server.state).to_keep.get_mut(&session) {
        list.retain(|k| k.prompt != prompt);
    }
}

/// What is known of `prompt`'s answer for `session`'s ssh to `host`. Asked again by the same
/// ssh, what it was given was wrong: forgotten.
fn known(server: &Server, session: u64, host: &str, prompt: &str) -> Known {
    let mut state = lock(&server.state);
    let key = (host.to_owned(), prompt.to_owned());
    let once = (session, prompt.to_owned());
    if state.answered_from_known.remove(&once) {
        state.known.remove(&key);
        invalidate_host(host);
        return Known::Wrong;
    }
    match state.known.get(&key).cloned() {
        Some(secret) => {
            state.answered_from_known.insert(once);
            Known::Use(secret)
        }
        None => Known::Nothing,
    }
}

/// Keeps `secret` for the machine's other shells, and marks it given to this ssh: it asking
/// again means it was wrong (see [`known`]).
fn remember(server: &Server, session: u64, host: &str, prompt: &str, secret: &str) {
    let mut state = lock(&server.state);
    if state
        .known
        .insert((host.to_owned(), prompt.to_owned()), secret.to_owned())
        .is_some_and(|old| old != secret)
    {
        invalidate_host(host);
    }
    state
        .answered_from_known
        .insert((session, prompt.to_owned()));
}

/// Whether two tokens are the same, in a time that doesn't tell how much of one matched.
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
mod tests {
    use super::super::vault;
    use super::*;

    /// What `env_for` gives ssh as `GITCRAFT_ASKPASS`.
    fn session_for(host: &str) -> String {
        start_for(host).0
    }

    /// An ssh started for `host`: what it is given as `GITCRAFT_ASKPASS`, and its attempt.
    fn start_for(host: &str) -> (String, Option<Attempt>) {
        match env_for(host) {
            Some((env, attempt)) => (
                env.into_iter()
                    .find(|(k, _)| *k == ENV)
                    .map(|(_, v)| v)
                    .unwrap_or_default(),
                Some(attempt),
            ),
            None => (String::new(), None),
        }
    }

    /// Answers `prompt` asked by the ssh `found` names with `secret`, ticking "Remember
    /// connection".
    fn type_and_remember(found: &str, prompt: &str, secret: &str) -> Option<String> {
        let asked = ask_in_background(found.to_owned(), prompt, Kind::Secret);
        if let Some(q) = loop_until_shown(prompt) {
            answer_and_remember(q.id, secret.to_owned());
        }
        asked.join().ok().flatten()
    }

    /// Answers the next question shown whose prompt is `prompt` with `answer`; the question.
    fn answer_shown(prompt: &str, answer_with: Option<&str>) -> Option<Question> {
        for _ in 0..500 {
            if let Some(q) = pending().filter(|q| q.prompt == prompt) {
                answer(q.id, answer_with.map(str::to_owned));
                return Some(q);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    fn ask_in_background(
        found: String,
        prompt: &str,
        kind: Kind,
    ) -> std::thread::JoinHandle<Option<String>> {
        let prompt = prompt.to_owned();
        std::thread::spawn(move || ask_app(&found, &prompt, kind).ok().flatten())
    }

    #[test]
    fn questions_come_to_the_window_and_answers_go_back() {
        let _asking = asking();
        let prompt = "me@one's password: (answers)";
        let asked = ask_in_background(session_for("SSH · one"), prompt, Kind::Secret);
        let shown = answer_shown(prompt, Some("s3cret"));
        assert_eq!(
            shown.map(|q| (q.host, q.kind)),
            Some(("SSH · one".to_owned(), Kind::Secret))
        );
        assert_eq!(asked.join().ok().flatten(), Some("s3cret".to_owned()));
        // Declined.
        let asked = ask_in_background(session_for("SSH · one"), "Trust (yes/no)?", Kind::TrustHost);
        assert!(answer_shown("Trust (yes/no)?", None).is_some());
        assert_eq!(asked.join().ok().flatten(), None);
    }

    /// The machine's other shells get the password typed; one that asks again (it was wrong)
    /// makes it asked of the user again.
    #[test]
    fn a_secret_typed_serves_the_machine_until_it_is_wrong() {
        let _asking = asking();
        let prompt = "me@two's password: (remembered)";
        let first = session_for("SSH · two");
        let asked = ask_in_background(first.clone(), prompt, Kind::Secret);
        answer_shown(prompt, Some("typed"));
        assert_eq!(asked.join().ok().flatten(), Some("typed".to_owned()));
        // Another shell of the machine: not asked.
        let other = session_for("SSH · two");
        assert_eq!(
            ask_app(&other, prompt, Kind::Secret).ok().flatten(),
            Some("typed".to_owned())
        );
        // That shell asks again: what was remembered is wrong, the user is asked.
        let again = ask_in_background(other, prompt, Kind::Secret);
        assert!(answer_shown(prompt, Some("right")).is_some());
        assert_eq!(again.join().ok().flatten(), Some("right".to_owned()));
    }

    /// What this run of the app knows of `host`'s secrets, gone (as after a restart).
    fn restart_for(host: &str) {
        if let Some(server) = server() {
            lock(&server.state).known.retain(|(h, _), _| h != host);
        }
    }

    /// Kept with "Remember connection" once its ssh connected, a secret is given after a
    /// restart without asking; wrong, it is forgotten there too, and the user told so.
    #[test]
    fn a_remembered_secret_outlives_the_app_until_it_is_wrong() {
        let _asking = asking();
        let (host, prompt) = ("SSH · four", "me@four's password: (kept)");
        let (found, attempt) = start_for(host);
        assert_eq!(
            type_and_remember(&found, prompt, "kept"),
            Some("kept".to_owned())
        );
        // Not kept before the connection is made.
        assert_eq!(vault::recall(host, prompt), None);
        if let Some(attempt) = attempt {
            attempt.connected();
        }
        assert_eq!(vault::recall(host, prompt), Some("kept".to_owned()));

        restart_for(host);
        let after = session_for(host);
        assert_eq!(
            ask_app(&after, prompt, Kind::Secret).ok().flatten(),
            Some("kept".to_owned())
        );
        // That ssh asks again: the kept one was wrong.
        let again = ask_in_background(after, prompt, Kind::Secret);
        let shown = loop_until_shown(prompt);
        assert_eq!(shown.as_ref().map(|q| q.forgotten), Some(true));
        assert_eq!(vault::recall(host, prompt), None);
        if let Some(q) = shown {
            answer(q.id, Some("right".to_owned()));
        }
        assert_eq!(again.join().ok().flatten(), Some("right".to_owned()));
        // Typed without remembering: not kept.
        assert_eq!(vault::recall(host, prompt), None);
    }

    /// A connection that isn't made keeps nothing, even ticked.
    #[test]
    fn a_secret_is_not_kept_for_a_connection_that_failed() {
        let _asking = asking();
        let (host, prompt) = ("SSH · five", "me@five's password: (failed)");
        let (found, attempt) = start_for(host);
        assert_eq!(
            type_and_remember(&found, prompt, "typo"),
            Some("typo".to_owned())
        );
        drop(attempt);
        assert_eq!(vault::recall(host, prompt), None);
    }

    /// Wrong, then right, in one connection: the right one is kept, the wrong one never.
    #[test]
    fn only_the_secret_that_signed_in_is_kept() {
        let _asking = asking();
        let (host, prompt) = ("SSH · six", "me@six's password: (retyped)");
        let (found, attempt) = start_for(host);
        assert_eq!(
            type_and_remember(&found, prompt, "wrong"),
            Some("wrong".to_owned())
        );
        // ssh asks again: that was wrong. Typed again, not ticked this time.
        let again = ask_in_background(found, prompt, Kind::Secret);
        if let Some(q) = loop_until_shown(prompt) {
            answer(q.id, Some("right".to_owned()));
        }
        assert_eq!(again.join().ok().flatten(), Some("right".to_owned()));
        if let Some(attempt) = attempt {
            attempt.connected();
        }
        assert_eq!(vault::recall(host, prompt), None);

        // Wrong ticked, then right ticked: the right one.
        let (found, attempt) = start_for(host);
        restart_for(host);
        assert_eq!(
            type_and_remember(&found, prompt, "wrong"),
            Some("wrong".to_owned())
        );
        assert_eq!(
            type_and_remember(&found, prompt, "right"),
            Some("right".to_owned())
        );
        if let Some(attempt) = attempt {
            attempt.connected();
        }
        assert_eq!(vault::recall(host, prompt), Some("right".to_owned()));
    }

    /// The next question shown whose prompt is `prompt`, unanswered.
    fn loop_until_shown(prompt: &str) -> Option<Question> {
        for _ in 0..500 {
            if let Some(q) = pending().filter(|q| q.prompt == prompt) {
                return Some(q);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// A request longer than any helper's is dropped once that much is read: never held whole,
    /// never answered, nothing asked.
    #[test]
    fn an_endless_request_is_cut_off() -> Result<(), String> {
        let _asking = asking();
        let found = session_for("SSH · seven");
        let port: u16 = found
            .split_whitespace()
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or("no port")?;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        // No newline, ever: more than the limit, then the end of what it sends.
        let endless = vec![b'a'; usize::try_from(MAX_REQUEST).unwrap_or(0) + 4096];
        let _ = stream.write_all(&endless);
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);
        assert!(reply.is_empty(), "{:?}", String::from_utf8_lossy(&reply));
        Ok(())
    }

    /// Without the token, nothing is asked and nothing answered.
    #[test]
    fn a_stranger_gets_no_answer() {
        let _asking = asking();
        let real = session_for("SSH · three");
        let mut parts = real.split_whitespace();
        let (port, session) = (
            parts.next().unwrap_or_default(),
            parts.nth(1).unwrap_or_default(),
        );
        let forged = format!("{port} 0000000000000000 {session}");
        let reply = ask_app(&forged, "me@three's password: (stranger)", Kind::Secret);
        assert!(matches!(reply, Ok(None) | Err(_)), "{reply:?}");
        assert!(pending().is_none_or(|q| !q.prompt.contains("(stranger)")));
    }

    #[test]
    fn what_ssh_asks_is_told_apart() {
        let _asking = asking();
        assert_eq!(Kind::of("me@h's password: ", None), Kind::Secret);
        assert_eq!(
            Kind::of("Enter passphrase for key 'k': ", None),
            Kind::Secret
        );
        assert_eq!(
            Kind::of(
                "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
                None
            ),
            Kind::TrustHost
        );
        assert_eq!(
            Kind::of("Allow use of key?", Some("confirm")),
            Kind::Confirm
        );
        assert_eq!(
            Kind::of("Confirm user presence", Some("none")),
            Kind::Notice
        );
    }
}
