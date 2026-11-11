//! Owned MCP child transport, repository setup, and window listener fixtures.
use super::Result;
use gitcraft::mcp::link::{self, Incoming};
use gitcraft::mcp::wire::{Reply, WriteOp};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

pub(super) fn git(dir: &Path, args: &[&str]) -> Result {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", dir.join("no-global-config"))
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// A repository with two commits, a branch and an unstaged change, in a fresh folder.
pub(super) fn repo(name: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("gitcraft-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    git(&dir, &["init", "-q", "-b", "main"])?;
    git(&dir, &["config", "user.name", "Ada"])?;
    git(&dir, &["config", "user.email", "ada@example.com"])?;
    git(&dir, &["config", "commit.gpgsign", "false"])?;
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").map_err(|e| e.to_string())?;
    git(&dir, &["add", "a.txt"])?;
    git(&dir, &["commit", "-q", "-m", "First"])?;
    git(&dir, &["branch", "side"])?;
    std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").map_err(|e| e.to_string())?;
    git(&dir, &["commit", "-q", "-am", "Second"])?;
    std::fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The server, and its replies line by line.
pub(super) struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

impl Server {
    pub(super) fn close_stdin(&mut self) {
        self.stdin = None;
    }
    pub(super) fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    pub(super) fn start(repo: &Path, windows: &Path) -> Result<Self> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_gitcraft"))
            .arg("mcp")
            .current_dir(repo)
            .env("GITCRAFT_MCP_DIR", windows)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start gitcraft mcp: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            lines,
            next_id: 1,
        })
    }

    pub(super) fn send(&mut self, message: &Value) -> Result {
        let mut line = message.to_string();
        line.push('\n');
        let stdin = self.stdin.as_mut().ok_or("stdin is closed")?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| e.to_string())
    }

    /// Sends a request and returns the response with its id (skipping notifications).
    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(30))
                .map_err(|_| format!("no answer to {method}"))?;
            let value: Value =
                serde_json::from_str(&line).map_err(|e| format!("not JSON ({e}): {line}"))?;
            if value["id"] == json!(id) {
                return Ok(value);
            }
        }
    }

    pub(super) fn initialize(&mut self) -> Result<Value> {
        let reply = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "claude-code", "version": "2.0.0"},
            }),
        )?;
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(reply)
    }

    /// Calls `tool`; returns whether it failed and its text.
    pub(super) fn call(&mut self, tool: &str, arguments: Value) -> Result<(bool, String)> {
        let reply = self.request("tools/call", json!({"name": tool, "arguments": arguments}))?;
        let result = &reply["result"];
        if result.is_null() {
            return Err(format!("{tool}: no result: {reply}"));
        }
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        Ok((result["isError"] == json!(true), text))
    }

    /// Calls a reading tool that must succeed; its answer as JSON.
    pub(super) fn read(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        let (failed, text) = self.call(tool, arguments)?;
        if failed {
            return Err(format!("{tool} failed: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("{tool}: not JSON ({e}): {text}"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn check(ok: bool, what: &str) -> Result {
    if ok { Ok(()) } else { Err(what.to_owned()) }
}

pub(super) fn empty_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// What the stand-in window was asked: the repository, the agent and the operation.
pub(super) type Asked = Receiver<(String, String, WriteOp)>;

/// A stand-in window: the channel's real listener, answering every request with `answer` and
/// passing on what it was asked.
pub(super) fn window(dir: &Path, answer: Reply) -> Result<(link::Listener, Asked)> {
    let (tx, asked) = channel();
    let tx = std::sync::Mutex::new(tx);
    let listener = link::listen(dir, move |incoming: Incoming| {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send((incoming.repo, incoming.agent, incoming.op));
        }
        let _ = incoming.reply.send(answer.clone());
    })?;
    Ok((listener, asked))
}
