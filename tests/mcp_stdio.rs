//! `gitcraft mcp` as an agent runs it: the binary started with pipes for stdin and stdout, in a
//! real repository, driven through initialize, tools/list and tools/call. Changing tools are
//! tried with no window running, and with a stand-in window (the channel's real listener) that
//! answers as gitcraft's does.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use gitcraft::mcp::link::{self, Incoming};
use gitcraft::mcp::wire::{Outcome, Reply, WriteOp};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, String>;

fn git(dir: &Path, args: &[&str]) -> Result {
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
fn repo(name: &str) -> Result<PathBuf> {
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
struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

impl Server {
    fn start(repo: &Path, windows: &Path) -> Result<Self> {
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
            stdin,
            lines,
            next_id: 1,
        })
    }

    fn send(&mut self, message: &Value) -> Result {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|e| e.to_string())
    }

    /// Sends a request and returns the response with its id (skipping notifications).
    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
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

    fn initialize(&mut self) -> Result<Value> {
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
    fn call(&mut self, tool: &str, arguments: Value) -> Result<(bool, String)> {
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
    fn read(&mut self, tool: &str, arguments: Value) -> Result<Value> {
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

fn check(ok: bool, what: &str) -> Result {
    if ok { Ok(()) } else { Err(what.to_owned()) }
}

fn empty_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn an_agent_reads_the_repository_over_stdio() -> Result {
    let dir = repo("read")?;
    let windows = empty_dir("read-windows");
    let outcome = read_everything(&dir, &windows);
    let _ = std::fs::remove_dir_all(&dir);
    outcome
}

fn read_everything(dir: &Path, windows: &Path) -> Result {
    let mut server = Server::start(dir, windows)?;
    let init = server.initialize()?;
    let result = &init["result"];
    check(result["serverInfo"]["name"] == "gitcraft", "server name")?;
    check(
        result["protocolVersion"] == "2025-06-18",
        "protocol version",
    )?;
    check(
        result["capabilities"]["tools"].is_object(),
        "tools capability",
    )?;

    let listed = server.request("tools/list", json!({}))?;
    let tools = listed["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for name in [
        "log",
        "show_commit",
        "blame",
        "status",
        "diff",
        "conflicts",
        "branches",
        "ci_status",
        "pull_requests",
        "stage",
        "commit",
        "delete_branch",
        "push",
    ] {
        check(names.contains(&name), &format!("{name} listed"))?;
    }
    let log_tool = tools.iter().find(|t| t["name"] == "log");
    check(
        log_tool.is_some_and(|t| t["annotations"]["readOnlyHint"] == true),
        "log is read-only",
    )?;
    check(
        log_tool.is_some_and(|t| t["inputSchema"]["type"] == "object"),
        "log has an object schema",
    )?;

    let status = server.read("status", json!({}))?;
    check(status["branch"]["name"] == "main", "status branch")?;
    check(
        status["unstaged"][0]["path"] == "a.txt",
        "status unstaged file",
    )?;

    let log = server.read("log", json!({"limit": 1}))?;
    check(log["commits"][0]["summary"] == "Second", "newest commit")?;
    check(log["next_skip"] == 1, "a next page")?;
    let older = server.read("log", json!({"limit": 5, "skip": 1}))?;
    check(older["commits"][0]["summary"] == "First", "second page")?;
    let refs = older["commits"][0]["refs"].to_string();
    check(refs.contains("side"), "branches on the commits")?;

    let commit = server.read("show_commit", json!({"commit": "HEAD"}))?;
    check(
        commit["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("Second")),
        "message",
    )?;
    check(commit["files"][0]["path"] == "a.txt", "changed file")?;

    let diff = server.read("diff", json!({}))?;
    check(
        diff["text"].as_str().is_some_and(|t| t.contains("+TWO")),
        "working tree diff",
    )?;
    let paged = server.read("diff", json!({"commit": "HEAD", "max_bytes": 1000}))?;
    check(
        paged["text"].as_str().is_some_and(|t| t.contains("+three")),
        "commit diff",
    )?;

    let blame = server.read(
        "blame",
        json!({"path": "a.txt", "start_line": 3, "end_line": 3}),
    )?;
    check(blame["lines"][0]["text"] == "three", "blamed line")?;
    check(blame["lines"][0]["author"] == "Ada", "blamed author")?;

    let branches = server.read("branches", json!({}))?;
    check(branches["current"] == "main", "current branch")?;
    check(
        branches["local"].as_array().is_some_and(|l| l.len() == 2),
        "two branches",
    )?;

    let conflicts = server.read("conflicts", json!({}))?;
    check(
        conflicts["conflicted"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "no conflicts",
    )?;

    // Wrong arguments: a result the agent reads, not a protocol error.
    let (failed, text) = server.call("blame", json!({"path": "../../etc/passwd"}))?;
    check(
        failed && text.contains("inside the repository"),
        "path outside refused",
    )?;
    let (failed, text) = server.call("log", json!({"limit": 5000}))?;
    check(failed && text.contains("`limit`"), "limit refused")?;
    let (failed, text) = server.call("diff", json!({"commit": "--output=/tmp/x"}))?;
    check(
        failed && text.contains("not a revision"),
        "option-like revision refused",
    )?;
    let (failed, text) = server.call("status", json!({"verbose": true}))?;
    check(
        failed && text.contains("unknown argument"),
        "unknown argument refused",
    )?;
    // A forge-less repository: said, not crashed.
    let (failed, text) = server.call("pull_requests", json!({}))?;
    check(failed && text.contains("forge"), "no forge")?;

    // An unknown tool is a protocol error (-32602).
    let unknown = server.request("tools/call", json!({"name": "rm_rf", "arguments": {}}))?;
    check(
        unknown["error"]["code"] == -32602,
        "unknown tool error code",
    )?;
    // A method that doesn't exist.
    let missing = server.request("resources/bogus", json!({}))?;
    check(
        missing["error"]["code"] == -32601,
        "unknown method error code",
    )?;
    Ok(())
}

#[test]
fn without_a_window_changes_are_refused_with_what_to_do() -> Result {
    let dir = repo("no-window")?;
    let windows = empty_dir("no-window-windows");
    let outcome = (|| {
        let mut server = Server::start(&dir, &windows)?;
        server.initialize()?;
        let (failed, text) = server.call("stage", json!({"paths": ["a.txt"]}))?;
        check(failed, "stage without a window fails")?;
        check(
            text.contains("gitcraft isn't running"),
            &format!("says why: {text}"),
        )?;
        // Nothing was staged.
        let status = server.read("status", json!({}))?;
        check(
            status["staged"].as_array().is_some_and(Vec::is_empty),
            "nothing staged",
        )
    })();
    let _ = std::fs::remove_dir_all(&dir);
    outcome
}

/// What the stand-in window was asked: the repository, the agent and the operation.
type Asked = Receiver<(String, String, WriteOp)>;

/// A stand-in window: the channel's real listener, answering every request with `answer` and
/// passing on what it was asked.
fn window(dir: &Path, answer: Reply) -> Result<(link::Listener, Asked)> {
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

#[test]
fn a_change_goes_to_the_window_with_the_agents_name() -> Result {
    let dir = repo("window")?;
    let windows = empty_dir("window-windows");
    let outcome = (|| {
        let (listener, asked) = window(&windows, Reply::new(Outcome::Done, "Stage a.txt: done."))?;
        let mut server = Server::start(&dir, &windows)?;
        server.initialize()?;
        let (failed, text) = server.call("stage", json!({"paths": ["a.txt"]}))?;
        drop(listener);
        check(
            !failed && text.contains("Stage a.txt: done."),
            &format!("done: {text}"),
        )?;
        let (repo, agent, op) = asked
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "the window wasn't asked")?;
        check(agent == "Claude Code", &format!("agent named: {agent}"))?;
        check(
            Path::new(&repo).file_name() == dir.file_name(),
            &format!("the repository: {repo}"),
        )?;
        check(
            op == WriteOp::Stage {
                paths: vec!["a.txt".into()],
            },
            "the operation",
        )
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&windows);
    outcome
}

#[test]
fn with_changes_turned_off_the_agent_is_told() -> Result {
    let dir = repo("disabled")?;
    let windows = empty_dir("disabled-windows");
    let outcome = (|| {
        let (listener, _asked) = window(
            &windows,
            Reply::new(
                Outcome::Disabled,
                "The user turned off changes by agents in gitcraft (Settings → Agents).",
            ),
        )?;
        let mut server = Server::start(&dir, &windows)?;
        server.initialize()?;
        let (failed, text) = server.call("commit", json!({"message": "x"}))?;
        let (denied, denial) = {
            drop(listener);
            let (listener, _asked) = window(
                &windows,
                Reply::new(Outcome::Denied, "The user chose Deny."),
            )?;
            let answer = server.call("push", json!({"force": true}))?;
            drop(listener);
            answer
        };
        check(
            failed && text.contains("Settings → Agents"),
            &format!("disabled: {text}"),
        )?;
        check(
            denied && denial.contains("denied"),
            &format!("denied: {denial}"),
        )
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&windows);
    outcome
}
