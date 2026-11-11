//! `gitcraft mcp` as an agent runs it: the binary started with pipes for stdin and stdout, in a
//! real repository, driven through initialize, tools/list and tools/call. Changing tools are
//! tried with no window running, and with a stand-in window (the channel's real listener) that
//! answers as gitcraft's does.

use std::path::Path;
use std::time::Duration;

use gitcraft::mcp::link::{self, Incoming};
use gitcraft::mcp::wire::{Outcome, Reply, WriteOp};
use serde_json::json;

type Result<T = ()> = std::result::Result<T, String>;

#[path = "support/mcp_stdio.rs"]
mod support;
use support::*;

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

#[test]
fn huge_content_comes_back_bounded() -> Result {
    let dir = repo("huge")?;
    let windows = empty_dir("huge-windows");
    let outcome = (|| {
        std::fs::write(dir.join("min.js"), "x".repeat(2_000_000)).map_err(|e| e.to_string())?;
        git(&dir, &["add", "min.js"])?;
        let message = dir.join("message.txt");
        std::fs::write(&message, format!("Big\n\n{}", "y".repeat(1_000_000)))
            .map_err(|e| e.to_string())?;
        git(&dir, &["commit", "-q", "-F", &message.to_string_lossy()])?;
        let mut server = Server::start(&dir, &windows)?;
        server.initialize()?;
        for (tool, args) in [
            ("blame", json!({"path": "min.js"})),
            ("show_commit", json!({})),
            ("log", json!({"limit": 1})),
            ("diff", json!({"commit": "HEAD"})),
        ] {
            let (failed, text) = server.call(tool, args)?;
            check(
                !failed,
                &format!("{tool} answers: {}", &text[..text.len().min(200)]),
            )?;
            check(
                text.len() < 300_000,
                &format!("{tool} is {} bytes", text.len()),
            )?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    outcome
}

#[test]
fn closing_stdin_ends_a_wait_for_the_window() -> Result {
    let dir = repo("cancel")?;
    let windows = empty_dir("cancel-windows");
    let outcome = (|| {
        // A window that takes the request and never answers it.
        let held = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Incoming>::new()));
        let keep = held.clone();
        let _listener = link::listen(&windows, move |incoming: Incoming| {
            if let Ok(mut held) = keep.lock() {
                held.push(incoming);
            }
        })?;
        let mut server = Server::start(&dir, &windows)?;
        server.initialize()?;
        server.send(&json!({"jsonrpc": "2.0", "id": 99, "method": "tools/call",
            "params": {"name": "stage", "arguments": {"paths": ["a.txt"]}}}))?;
        let started = std::time::Instant::now();
        while held.lock().map(|h| h.is_empty()).unwrap_or(true) {
            check(
                started.elapsed() < Duration::from_secs(10),
                "the window got it",
            )?;
            std::thread::sleep(Duration::from_millis(20));
        }
        // The agent goes away.
        server.close_stdin();
        let started = std::time::Instant::now();
        loop {
            if server.has_exited() {
                break;
            }
            check(
                started.elapsed() < Duration::from_secs(10),
                "gitcraft mcp ends",
            )?;
            std::thread::sleep(Duration::from_millis(50));
        }
        // And the window learns it: the request is dropped, not run.
        let started = std::time::Instant::now();
        loop {
            let gone = held.lock().map(|h| {
                h.iter()
                    .all(|i| i.gone.load(std::sync::atomic::Ordering::SeqCst))
            });
            if gone.unwrap_or(false) {
                return Ok(());
            }
            check(
                started.elapsed() < Duration::from_secs(5),
                "the window sees it gone",
            )?;
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&windows);
    outcome
}
