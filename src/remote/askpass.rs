//! ssh's questions, asked in the window: a password, a key's passphrase, whether to trust a
//! host's key seen for the first time.
//!
//! ssh is started with this app as its `SSH_ASKPASS` program. Started so, the app doesn't open
//! a window (see [`run_helper`]): it hands the question to the running app over a connection
//! to 127.0.0.1, with a token only the ssh it started knows, and prints the answer for ssh.
//! The running app shows the question (see [`pending`] and [`answer`]) and sends the answer
//! back. A password or passphrase typed is remembered for the machine while the app runs, so
//! the other shells kept open there don't ask again; one ssh asks again, and it was wrong: it
//! is forgotten, and the question shown again.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
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
}

struct Waiting {
    question: Question,
    reply: Sender<Option<String>>,
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
    repaint: Option<egui::Context>,
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
fn random_token() -> String {
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

/// The program and environment an ssh connecting to `host` is started with, so that its
/// questions come to the window; `None` when there's no way to (no listener could be made).
pub(super) fn env_for(host_label: &str) -> Option<Vec<(&'static str, String)>> {
    static SESSIONS: AtomicU64 = AtomicU64::new(1);
    let server = server()?;
    let program = std::env::current_exe().ok()?;
    let session = SESSIONS.fetch_add(1, Ordering::Relaxed);
    lock(&server.state)
        .sessions
        .insert(session, host_label.to_owned());
    Some(vec![
        ("SSH_ASKPASS", program.display().to_string()),
        ("SSH_ASKPASS_REQUIRE", "force".to_owned()),
        (ENV, format!("{} {} {session}", server.port, server.token)),
    ])
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
    let Some(server) = server() else { return };
    let mut state = lock(&server.state);
    state.last_asked = Some(Instant::now());
    if let Some(i) = state.waiting.iter().position(|w| w.question.id == id) {
        let waiting = state.waiting.remove(i);
        let _ = waiting.reply.send(answer);
    }
}

/// One question from a helper: `<token> <session> <kind> <prompt in base64>`, answered
/// `OK <answer in base64>` or `NO`.
fn serve(stream: TcpStream) -> std::io::Result<()> {
    static IDS: AtomicU64 = AtomicU64::new(1);
    let Some(server) = server() else {
        return Ok(());
    };
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
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

/// The answer to `prompt` of `session`'s ssh: a secret known for its machine, else the user's.
fn ask(server: &Server, session: u64, kind: Kind, prompt: String, id: u64) -> Option<String> {
    let (rx, repaint, key) = {
        let mut state = lock(&server.state);
        let host = state
            .sessions
            .get(&session)
            .cloned()
            .unwrap_or_else(|| "SSH".to_owned());
        let key = (host.clone(), prompt.clone());
        let once = (session, prompt.clone());
        if kind == Kind::Secret {
            if state.answered_from_known.contains(&once) {
                // Asked again by the same ssh: what was known didn't do.
                state.known.remove(&key);
                state.answered_from_known.remove(&once);
            } else if let Some(known) = state.known.get(&key).cloned() {
                state.answered_from_known.insert(once);
                return Some(known);
            }
        }
        let (tx, rx) = channel();
        state.waiting.push(Waiting {
            question: Question {
                id,
                host,
                prompt,
                kind,
            },
            reply: tx,
        });
        state.last_asked = Some(Instant::now());
        (rx, state.repaint.clone(), key)
    };
    if let Some(ctx) = repaint {
        ctx.request_repaint();
    }
    let answer = rx.recv().ok().flatten();
    // Kept for the machine's other shells, and marked as given to this ssh: it asking again
    // means it was wrong (see above).
    if kind == Kind::Secret
        && let Some(secret) = &answer
    {
        let mut state = lock(&server.state);
        state.known.insert(key.clone(), secret.clone());
        state.answered_from_known.insert((session, key.1));
    }
    answer
}

/// Whether two tokens are the same, in a time that doesn't tell how much of one matched.
fn same(a: &[u8], b: &[u8]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What `env_for` gives ssh as `GITCRAFT_ASKPASS`.
    fn session_for(host: &str) -> String {
        env_for(host)
            .and_then(|env| env.into_iter().find(|(k, _)| *k == ENV).map(|(_, v)| v))
            .unwrap_or_default()
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

    /// Without the token, nothing is asked and nothing answered.
    #[test]
    fn a_stranger_gets_no_answer() {
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
