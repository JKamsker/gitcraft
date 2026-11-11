//! A small LFS server for tests: the locks API (create, list, verify, unlock) over plain HTTP
//! on a local port, enough for `git lfs lock`, `locks` and `unlock` to talk to. Locks it is
//! told about up front belong to someone else; those `git lfs lock` takes are "me".

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

#[derive(Clone)]
struct Lock {
    id: String,
    path: String,
    owner: String,
}

#[derive(Default)]
struct State {
    locks: Vec<Lock>,
    next: u64,
    /// Refuse `/locks/verify` (a server that lists locks but can't verify them).
    no_verify: bool,
}

pub(crate) struct LfsServer {
    pub(crate) url: String,
    state: Arc<Mutex<State>>,
}

impl LfsServer {
    pub(crate) fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = shared.clone();
                std::thread::spawn(move || serve(stream, &state));
            }
        });
        Ok(Self {
            url: format!("http://127.0.0.1:{port}/"),
            state,
        })
    }

    /// A lock someone else ("ana") holds.
    pub(crate) fn lock_for_someone_else(&self, path: &str) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.next += 1;
        let id = format!("L{}", s.next);
        s.locks.push(Lock {
            id,
            path: path.to_owned(),
            owner: "ana".to_owned(),
        });
    }

    pub(crate) fn refuse_verify(&self) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .no_verify = true;
    }

    pub(crate) fn paths(&self) -> Vec<String> {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.locks.iter().map(|l| l.path.clone()).collect()
    }

    /// Points the repository at this server.
    pub(crate) fn serve_repo(&self, repo: &Path) -> Result<(), String> {
        super::run(Some(repo), &["config", "lfs.url", &self.url])?;
        super::run(Some(repo), &["config", "lfs.locksverify", "false"])?;
        Ok(())
    }
}

fn lock_json(l: &Lock) -> Value {
    json!({"id": l.id, "path": l.path, "owner": {"name": l.owner},
           "locked_at": "2026-11-01T10:00:00Z"})
}

fn serve(stream: TcpStream, state: &Mutex<State>) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    loop {
        let mut request = String::new();
        if reader.read_line(&mut request).unwrap_or(0) == 0 {
            return;
        }
        let mut length = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':')
                && k.eq_ignore_ascii_case("content-length")
            {
                length = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let mut words = request.split_whitespace();
        let (method, target) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        let path = target.split('?').next().unwrap_or("");
        let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let (code, reply) = answer(state, method, path, &body);
        let text = reply.to_string();
        let head = format!(
            "HTTP/1.1 {code} X\r\nContent-Type: application/vnd.git-lfs+json\r\n\
             Content-Length: {}\r\n\r\n",
            text.len()
        );
        if out.write_all(head.as_bytes()).is_err() || out.write_all(text.as_bytes()).is_err() {
            return;
        }
    }
}

fn answer(state: &Mutex<State>, method: &str, path: &str, body: &Value) -> (u16, Value) {
    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
    let path = path.trim_end_matches('/');
    if method == "GET" && path.ends_with("/locks") {
        let locks: Vec<Value> = s.locks.iter().map(lock_json).collect();
        return (200, json!({ "locks": locks }));
    }
    if method != "POST" {
        return (404, json!({"message": "Not found"}));
    }
    if path.ends_with("/locks/verify") {
        if s.no_verify {
            return (404, json!({"message": "Not found"}));
        }
        let (ours, theirs): (Vec<&Lock>, Vec<&Lock>) =
            s.locks.iter().partition(|l| l.owner == "me");
        let ours: Vec<Value> = ours.into_iter().map(lock_json).collect();
        let theirs: Vec<Value> = theirs.into_iter().map(lock_json).collect();
        return (200, json!({ "ours": ours, "theirs": theirs }));
    }
    if path.ends_with("/locks") {
        let wanted = body["path"].as_str().unwrap_or("").to_owned();
        if let Some(held) = s.locks.iter().find(|l| l.path == wanted) {
            let held = lock_json(held);
            return (409, json!({"lock": held, "message": "Lock exists"}));
        }
        s.next += 1;
        let lock = Lock {
            id: format!("L{}", s.next),
            path: wanted,
            owner: "me".to_owned(),
        };
        let reply = lock_json(&lock);
        s.locks.push(lock);
        return (201, json!({ "lock": reply }));
    }
    if let Some(rest) = path.strip_suffix("/unlock") {
        let id = rest.rsplit('/').next().unwrap_or("");
        let force = body["force"].as_bool().unwrap_or(false);
        let Some(at) = s.locks.iter().position(|l| l.id == id) else {
            return (404, json!({"message": "Lock not found"}));
        };
        if s.locks[at].owner != "me" && !force {
            return (
                403,
                json!({"message": "You must have admin access to force delete a lock"}),
            );
        }
        let lock = s.locks.remove(at);
        return (200, json!({ "lock": lock_json(&lock) }));
    }
    (404, json!({"message": "Not found"}))
}

/// A repository with git-lfs set up, served by a test LFS server, or why the test can't run.
pub(crate) fn lfs_repo(name: &str) -> Result<Option<(PathBuf, LfsServer)>, String> {
    if crate::git::run(None, &["lfs", "version"]).is_err() {
        eprintln!("skipped: git-lfs is not installed here");
        return Ok(None);
    }
    let dir = std::env::temp_dir().join(format!("gitcraft-lfs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| crate::git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["lfs", "install", "--local"],
    ] {
        git(args)?;
    }
    let server = LfsServer::start()?;
    server.serve_repo(&dir)?;
    Ok(Some((dir, server)))
}
