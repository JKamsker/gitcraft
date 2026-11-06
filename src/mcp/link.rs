//! The local channel between `gitcraft mcp` and a running window: the window listens on
//! 127.0.0.1, on a port of its own, and writes the port and a random token into a discovery
//! file only this user can read. A `gitcraft mcp` process reads those files and hands its
//! request to each window in turn, with that window's token, until one has the repository
//! open. Anything else that connects without the token is refused before its request is read
//! any further.
//!
//! One request line and one reply line ([`super::wire`]) per connection. The window answers
//! from a thread of the connection's own; the window's own thread only gets the request
//! through [`Incoming`], and never waits on the connection.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

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

/// How long a connection may take to send its request.
const READ_REQUEST: Duration = Duration::from_secs(10);

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
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (token, deliver) = (token.clone(), deliver.clone());
            std::thread::spawn(move || serve(stream, &token, &*deliver));
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

/// One line, its `\n` included; `None` when longer than [`MAX_LINE`], cut short or not text.
fn read_line(stream: &TcpStream) -> Option<String> {
    let mut line = Vec::new();
    BufReader::new(stream.take(MAX_LINE))
        .read_until(b'\n', &mut line)
        .ok()?;
    if line.last() != Some(&b'\n') {
        return None;
    }
    String::from_utf8(line).ok()
}

fn send(mut stream: &TcpStream, reply: &Reply) {
    if let Ok(mut text) = serde_json::to_string(reply) {
        text.push('\n');
        let _ = stream.write_all(text.as_bytes());
    }
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

/// Answers one connection: its request checked, handed to `deliver`, the answer sent back.
fn serve(stream: TcpStream, token: &str, deliver: &(dyn Fn(Incoming) + Send + Sync)) {
    let _ = stream.set_read_timeout(Some(READ_REQUEST));
    let Some(line) = read_line(&stream) else {
        return;
    };
    let request = match check(&line, token) {
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
                "gitcraft's window dropped the request (its tab was closed?)"
            }
        };
        Reply::new(Outcome::TimedOut, why)
    });
    send(&stream, &reply);
}

/// The request in `line`, if it has the right version and token.
fn check(line: &str, token: &str) -> Result<Request, Reply> {
    // The version and token are looked at before the rest is believed.
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|_| Reply::new(Outcome::BadRequest, "not a gitcraft request"))?;
    let given = value.get("token").and_then(|t| t.as_str()).unwrap_or("");
    if !same_token(given, token) {
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
            token: window.token.clone(),
            repo: repo.to_owned(),
            agent: agent.to_owned(),
            op: op.clone(),
        };
        match ask_one(window.port, &request, wait) {
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
    /// Nothing listens there any more.
    Gone,
    /// It took the request but didn't answer.
    NoAnswer(String),
}

fn ask_one(port: u16, request: &Request, wait: Duration) -> Result<Reply, Unreachable> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|_| Unreachable::Gone)?;
    let mut line =
        serde_json::to_string(request).map_err(|e| Unreachable::NoAnswer(e.to_string()))?;
    line.push('\n');
    (&stream)
        .write_all(line.as_bytes())
        .map_err(|e| Unreachable::NoAnswer(e.to_string()))?;
    let _ = stream.set_read_timeout(Some(wait));
    let answer = read_line(&stream).ok_or_else(|| {
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
    fn a_request_without_the_token_is_refused_before_it_is_delivered() {
        let token = "0123456789abcdef";
        let request = |token: &str, v: u32| {
            serde_json::to_string(&Request {
                v,
                token: token.into(),
                repo: "/r".into(),
                agent: "a".into(),
                op: stage(),
            })
            .unwrap_or_default()
        };
        let refused = |line: String| check(&line, token).err().map(|r| r.outcome);
        assert_eq!(refused(request("wrong", 1)), Some(Outcome::BadRequest));
        assert_eq!(refused(request("", 1)), Some(Outcome::BadRequest));
        assert_eq!(refused(request(token, 2)), Some(Outcome::BadRequest));
        assert_eq!(refused("{not json".into()), Some(Outcome::BadRequest));
        assert_eq!(
            refused(r#"{"v":1,"token":"0123456789abcdef","op":{"op":"rm_rf"}}"#.into()),
            Some(Outcome::BadRequest)
        );
        assert!(check(&request(token, 1), token).is_ok());
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
