//! The local channel between `gitcraft mcp` and a running window: the window listens on
//! 127.0.0.1, on a port of its own, and writes the port and a random token into a discovery
//! file only this user can read. A `gitcraft mcp` process reads those files and hands its
//! request to each window in turn until one has the repository open.
//!
//! The token itself never crosses the connection. Each side proves it holds it on the other's
//! random challenge: the window first (so that whatever took the port of a window that crashed
//! learns nothing of the request), then the request, which is refused before it is read any
//! further without its proof.
//!
//! Per connection: a hello and the window's challenge, then one request line and one reply
//! line ([`super::wire`]). The window answers
//! from a thread of the connection's own; the window's own thread only gets the request
//! through [`Incoming`], and never waits on the connection.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::wire::{Outcome, Reply, Request, VERSION, WriteOp};

/// The folder the discovery files are in, unless `GITCRAFT_MCP_DIR` names another (the tests
/// do, to have a world of their own).
pub fn dir() -> Option<PathBuf> {
    match std::env::var_os("GITCRAFT_MCP_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => eframe::storage_dir("gitcraft").map(|d| d.join("agents")),
    }
}

/// The longest request or reply line: a commit message, a few hundred paths. What is longer
/// is refused, never held.
const MAX_LINE: u64 = 1024 * 1024;

/// How long a connection may take to send its hello, and then its request, each in whole.
const READ_REQUEST: Duration = Duration::from_secs(10);

/// The most connections a window serves at once: more are closed unread.
const MAX_CONNECTIONS: usize = 16;

/// The first line of a connection: who connects, and its challenge to the window.
#[derive(Serialize, Deserialize)]
struct Hello {
    v: u32,
    challenge: String,
}

/// The window's answer to a hello: its proof, and its own challenge.
#[derive(Serialize, Deserialize)]
struct Challenge {
    v: u32,
    proof: String,
    challenge: String,
}

/// Proves holding `token` on `challenge`, for `side` ("window" or "agent"): what the other
/// side can check without the token ever being sent.
fn proof(token: &str, challenge: &str, side: &str) -> String {
    format!(
        "{:x}",
        md5::compute(format!("{side}\0{token}\0{challenge}"))
    )
}

/// How long the window may take to answer: an operation that runs long (a push), or an
/// approval card waiting for the user, ends before this (see `repo::agent`).
pub const ANSWER_WITHIN: Duration = Duration::from_secs(15 * 60);

/// What a discovery file holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Discovery {
    v: u32,
    port: u16,
    token: String,
    pid: u32,
}

/// A request that came in, for the window to answer through `reply`. Dropping `reply`
/// unanswered tells the agent the window couldn't take it.
pub struct Incoming {
    pub repo: String,
    pub agent: String,
    pub op: WriteOp,
    pub reply: Sender<Reply>,
}

/// The window's end, listening while it lives: dropping it takes its discovery file away.
pub struct Listener {
    file: PathBuf,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Starts listening, with a discovery file in `dir`, handing each request to `deliver`.
pub fn listen(
    dir: &Path,
    deliver: impl Fn(Incoming) + Send + Sync + 'static,
) -> Result<Listener, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| format!("could not listen on 127.0.0.1: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("could not read the port listened on: {e}"))?
        .port();
    let token = crate::remote::askpass::random_token();
    let file = dir.join(format!("window-{}.json", std::process::id()));
    write_private(
        dir,
        &file,
        &Discovery {
            v: VERSION,
            port,
            token: token.clone(),
            pid: std::process::id(),
        },
    )?;
    let deliver = std::sync::Arc::new(deliver);
    let open = std::sync::Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                // Closed unread: whoever opens this many isn't a window's agent.
                open.fetch_sub(1, Ordering::SeqCst);
                continue;
            }
            let (token, deliver, open) = (token.clone(), deliver.clone(), open.clone());
            std::thread::spawn(move || {
                serve(stream, &token, &*deliver);
                open.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Ok(Listener { file })
}

/// Writes the discovery file readable by this user only, whole or not at all.
fn write_private(dir: &Path, file: &Path, discovery: &Discovery) -> Result<(), String> {
    let text = serde_json::to_string(discovery).map_err(|e| e.to_string())?;
    create_private_dir(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    let part = file.with_extension("part");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options
        .open(&part)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .and_then(|()| std::fs::rename(&part, file));
    written.map_err(|e| format!("could not write {}: {e}", file.display()))
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// One line, its `\n` included, read within `within` in whole; `None` when longer than
/// [`MAX_LINE`], cut short, too slow or not text.
fn read_line(stream: &TcpStream, within: Duration) -> Option<String> {
    let until = Instant::now() + within;
    let mut line = Vec::new();
    let mut byte = [0u8; 4096];
    let mut reader = stream;
    loop {
        let left = until.checked_duration_since(Instant::now())?;
        stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        let n = reader.read(&mut byte).ok()?;
        if n == 0 {
            return None;
        }
        let chunk = &byte[..n];
        match chunk.iter().position(|&b| b == b'\n') {
            Some(end) => {
                line.extend_from_slice(&chunk[..=end]);
                break;
            }
            None => line.extend_from_slice(chunk),
        }
        if line.len() as u64 > MAX_LINE {
            return None;
        }
    }
    if line.len() as u64 > MAX_LINE {
        return None;
    }
    String::from_utf8(line).ok()
}

/// Writes `value` as one line.
fn write_line(mut stream: &TcpStream, value: &impl Serialize) -> bool {
    match serde_json::to_string(value) {
        Ok(mut text) => {
            text.push('\n');
            stream.write_all(text.as_bytes()).is_ok()
        }
        Err(_) => false,
    }
}

fn send(stream: &TcpStream, reply: &Reply) {
    write_line(stream, reply);
}

/// Compares the token in time that doesn't depend on where they differ.
fn same_token(given: &str, token: &str) -> bool {
    given.len() == token.len()
        && given
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

/// Answers one connection: the window proves itself, the request is checked, handed to
/// `deliver`, and the answer sent back.
fn serve(stream: TcpStream, token: &str, deliver: &(dyn Fn(Incoming) + Send + Sync)) {
    let Some(hello) =
        read_line(&stream, READ_REQUEST).and_then(|line| serde_json::from_str::<Hello>(&line).ok())
    else {
        return;
    };
    // Any challenge but a sensible one is no agent's.
    if hello.challenge.len() < 16 || hello.challenge.len() > 128 {
        return;
    }
    let mine = crate::remote::askpass::random_token();
    let challenge = Challenge {
        v: VERSION,
        proof: proof(token, &hello.challenge, "window"),
        challenge: mine.clone(),
    };
    if !write_line(&stream, &challenge) {
        return;
    }
    let Some(line) = read_line(&stream, READ_REQUEST) else {
        return;
    };
    let request = match check(&line, &proof(token, &mine, "agent")) {
        Ok(request) => request,
        Err(reply) => return send(&stream, &reply),
    };
    let (tx, rx) = channel();
    deliver(Incoming {
        repo: request.repo,
        agent: request.agent,
        op: request.op,
        reply: tx,
    });
    let reply = rx.recv_timeout(ANSWER_WITHIN).unwrap_or_else(|e| {
        let why = match e {
            std::sync::mpsc::RecvTimeoutError::Timeout => "gitcraft took too long to answer",
            std::sync::mpsc::RecvTimeoutError::Disconnected => {
                "The repository's tab in gitcraft was closed before it was done; nothing more \
                 will happen."
            }
        };
        Reply::new(Outcome::TimedOut, why)
    });
    send(&stream, &reply);
}

/// The request in `line`, if it has the right version and `expected` as its proof.
fn check(line: &str, expected: &str) -> Result<Request, Reply> {
    // The version and proof are looked at before the rest is believed.
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|_| Reply::new(Outcome::BadRequest, "not a gitcraft request"))?;
    let given = value.get("proof").and_then(|t| t.as_str()).unwrap_or("");
    if !same_token(given, expected) {
        return Err(Reply::new(Outcome::BadRequest, "wrong token"));
    }
    let v = value.get("v").and_then(|v| v.as_u64()).unwrap_or(0);
    if v != u64::from(VERSION) {
        return Err(Reply::new(
            Outcome::BadRequest,
            format!(
                "this gitcraft speaks version {VERSION} of the agent format, not {v}: \
                 update gitcraft so that both are the same"
            ),
        ));
    }
    serde_json::from_value(value)
        .map_err(|e| Reply::new(Outcome::BadRequest, format!("malformed request: {e}")))
}

/// The windows' discovery files in `dir`, newest first.
fn windows(dir: &Path) -> Vec<(PathBuf, Discovery)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf, Discovery)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("window-") && name.ends_with(".json")
        })
        .filter_map(|e| {
            let path = e.path();
            let text = std::fs::read_to_string(&path).ok()?;
            let discovery: Discovery = serde_json::from_str(&text).ok()?;
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            Some((modified, path, discovery))
        })
        .collect();
    found.sort_by_key(|(modified, ..)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, p, d)| (p, d)).collect()
}

/// Asks the windows listed in `dir`, newest first, to run `op` in `repo` for `agent`; the
/// first that has `repo` open answers. `wait` bounds how long one answer may take.
pub fn ask(dir: &Path, repo: &str, agent: &str, op: &WriteOp, wait: Duration) -> Reply {
    let mut reached = false;
    for (file, window) in windows(dir) {
        let request = Request {
            v: VERSION,
            proof: String::new(),
            repo: repo.to_owned(),
            agent: agent.to_owned(),
            op: op.clone(),
        };
        match ask_one(&window, request, wait) {
            Err(Unreachable::Gone) => {
                // A window that ended without taking its file away (it crashed).
                let _ = std::fs::remove_file(&file);
            }
            Err(Unreachable::NoAnswer(e)) => {
                return Reply::new(Outcome::Failed, format!("gitcraft didn't answer: {e}"));
            }
            Ok(reply) if reply.outcome == Outcome::NotOpen => reached = true,
            Ok(reply) => return reply,
        }
    }
    let message = if reached {
        format!(
            "No gitcraft window has {repo} open. Ask the user to open it in gitcraft, then try \
             again."
        )
    } else {
        format!(
            "gitcraft isn't running. Ask the user to start gitcraft and open {repo} in it, then \
             try again."
        )
    };
    Reply::new(Outcome::NotOpen, message)
}

enum Unreachable {
    /// Nothing listens there any more, or what does isn't that window.
    Gone,
    /// It took the request but didn't answer.
    NoAnswer(String),
}

/// Hands `request` to `window`, once it has proved it holds its token.
fn ask_one(window: &Discovery, mut request: Request, wait: Duration) -> Result<Reply, Unreachable> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, window.port));
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|_| Unreachable::Gone)?;
    let mine = crate::remote::askpass::random_token();
    let hello = Hello {
        v: VERSION,
        challenge: mine.clone(),
    };
    if !write_line(&stream, &hello) {
        return Err(Unreachable::Gone);
    }
    let challenge = read_line(&stream, READ_REQUEST)
        .and_then(|line| serde_json::from_str::<Challenge>(&line).ok())
        .ok_or(Unreachable::Gone)?;
    // Not the window that wrote the file (it ended, and something else took its port): it
    // learns nothing of the request.
    if !same_token(&challenge.proof, &proof(&window.token, &mine, "window")) {
        return Err(Unreachable::Gone);
    }
    request.proof = proof(&window.token, &challenge.challenge, "agent");
    if !write_line(&stream, &request) {
        return Err(Unreachable::NoAnswer("the connection closed".to_owned()));
    }
    let answer = read_line(&stream, wait).ok_or_else(|| {
        Unreachable::NoAnswer("the connection ended without an answer".to_owned())
    })?;
    serde_json::from_str(&answer).map_err(|e| Unreachable::NoAnswer(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitcraft-link-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn stage() -> WriteOp {
        WriteOp::Stage {
            paths: vec!["a.txt".into()],
        }
    }

    #[test]
    fn a_request_reaches_the_window_and_its_answer_comes_back() {
        let dir = temp("answer");
        let listener = listen(&dir, |incoming: Incoming| {
            let message = format!("{} {} {:?}", incoming.agent, incoming.repo, incoming.op);
            let _ = incoming.reply.send(Reply::new(Outcome::Done, message));
        });
        assert!(listener.is_ok());
        let reply = ask(&dir, "/r", "Claude Code", &stage(), Duration::from_secs(5));
        drop(listener);
        let gone = !dir
            .join(format!("window-{}.json", std::process::id()))
            .exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::Done);
        assert!(reply.message.starts_with("Claude Code /r Stage"));
        assert!(gone, "the discovery file goes with the window");
    }

    #[test]
    fn without_a_window_the_agent_is_told_to_open_one() {
        let dir = temp("none");
        let reply = ask(&dir, "/r", "a", &stage(), Duration::from_secs(5));
        assert_eq!(reply.outcome, Outcome::NotOpen);
        assert!(reply.message.contains("isn't running"));
    }

    #[test]
    fn a_window_without_the_repository_passes_it_on() {
        let dir = temp("not-open");
        let listener = listen(&dir, |incoming: Incoming| {
            let _ = incoming.reply.send(Reply::new(Outcome::NotOpen, ""));
        });
        let reply = ask(&dir, "/r", "a", &stage(), Duration::from_secs(5));
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::NotOpen);
        assert!(reply.message.contains("No gitcraft window has /r open"));
    }

    #[test]
    fn a_stale_discovery_file_is_cleared_away() {
        let dir = temp("stale");
        // A port nothing listens on: bound, then let go.
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .unwrap_or(1);
        let file = dir.join("window-1.json");
        let stale = Discovery {
            v: VERSION,
            port,
            token: "t".into(),
            pid: 1,
        };
        assert!(write_private(&dir, &file, &stale).is_ok());
        let reply = ask(&dir, "/r", "a", &stage(), Duration::from_secs(5));
        let cleared = !file.exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::NotOpen);
        assert!(cleared);
    }

    #[test]
    fn a_request_without_its_proof_is_refused_before_it_is_delivered() {
        let expected = proof("0123456789abcdef", "challenge", "agent");
        let request = |proof: &str, v: u32| {
            serde_json::to_string(&Request {
                v,
                proof: proof.into(),
                repo: "/r".into(),
                agent: "a".into(),
                op: stage(),
            })
            .unwrap_or_default()
        };
        let refused = |line: String| check(&line, &expected).err().map(|r| r.outcome);
        assert_eq!(refused(request("wrong", 1)), Some(Outcome::BadRequest));
        assert_eq!(refused(request("", 1)), Some(Outcome::BadRequest));
        // The token itself is no proof.
        assert_eq!(
            refused(request("0123456789abcdef", 1)),
            Some(Outcome::BadRequest)
        );
        assert_eq!(refused(request(&expected, 2)), Some(Outcome::BadRequest));
        assert_eq!(refused("{not json".into()), Some(Outcome::BadRequest));
        let unknown = format!(r#"{{"v":1,"proof":"{expected}","op":{{"op":"rm_rf"}}}}"#);
        assert_eq!(refused(unknown), Some(Outcome::BadRequest));
        assert!(check(&request(&expected, 1), &expected).is_ok());
    }

    #[test]
    fn proofs_depend_on_the_token_the_challenge_and_the_side() {
        let p = proof("t", "c", "agent");
        assert_ne!(p, proof("u", "c", "agent"));
        assert_ne!(p, proof("t", "d", "agent"));
        assert_ne!(p, proof("t", "c", "window"));
        assert!(!p.contains('t') || p.len() == 32);
    }

    #[test]
    fn a_port_taken_over_after_a_crash_learns_nothing() {
        let dir = temp("impostor");
        // Something else listens where the file says the window is, without its token.
        let impostor = TcpListener::bind((Ipv4Addr::LOCALHOST, 0));
        let port = impostor
            .as_ref()
            .ok()
            .and_then(|l| l.local_addr().ok())
            .map(|a| a.port());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        if let Ok(impostor) = impostor {
            let seen = seen.clone();
            std::thread::spawn(move || {
                for stream in impostor.incoming().flatten().take(1) {
                    let hello = read_line(&stream, Duration::from_secs(5)).unwrap_or_default();
                    let fake = Challenge {
                        v: VERSION,
                        proof: "f".repeat(32),
                        challenge: "c".repeat(32),
                    };
                    write_line(&stream, &fake);
                    let rest = read_line(&stream, Duration::from_secs(2)).unwrap_or_default();
                    if let Ok(mut seen) = seen.lock() {
                        *seen = format!("{hello}{rest}");
                    }
                }
            });
        }
        let file = dir.join("window-1.json");
        let stale = Discovery {
            v: VERSION,
            port: port.unwrap_or(1),
            token: "the-real-windows-token".into(),
            pid: 1,
        };
        assert!(write_private(&dir, &file, &stale).is_ok());
        let reply = ask(&dir, "/secret/repo", "a", &stage(), Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(100));
        let seen = seen.lock().map(|s| s.clone()).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::NotOpen);
        assert!(!seen.contains("/secret/repo"), "{seen}");
        assert!(!seen.contains("the-real-windows-token"), "{seen}");
    }

    #[test]
    fn a_slow_or_endless_line_is_given_up_on() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0));
        let Ok(listener) = listener else { return };
        let Ok(address) = listener.local_addr() else {
            return;
        };
        std::thread::spawn(move || {
            if let Ok(mut s) = TcpStream::connect(address) {
                // One byte now and then, never a line's end.
                for _ in 0..50 {
                    if s.write_all(b"x").is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
            }
        });
        let Some(Ok(stream)) = listener.incoming().next() else {
            return;
        };
        let started = Instant::now();
        let line = read_line(&stream, Duration::from_millis(300));
        assert!(line.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the whole line has a deadline"
        );
    }

    #[test]
    fn tokens_are_compared_whole() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abd", "abc"));
        assert!(!same_token("ab", "abc"));
        assert!(!same_token("", "abc"));
    }

    #[cfg(unix)]
    #[test]
    fn only_this_user_can_read_the_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("private");
        let listener = listen(&dir, |_| {});
        let file = dir.join(format!("window-{}.json", std::process::id()));
        let mode = std::fs::metadata(&file).map(|m| m.permissions().mode() & 0o777);
        let dir_mode = std::fs::metadata(&dir).map(|m| m.permissions().mode() & 0o777);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(mode.ok(), Some(0o600));
        assert_eq!(dir_mode.ok(), Some(0o700));
    }
}
