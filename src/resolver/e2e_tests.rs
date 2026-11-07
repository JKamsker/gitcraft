//! A real agent resolving a real merge, its reports coming in over `gitcraft mcp` as they do in
//! the window. Not run by default: it needs the agent installed and signed in, takes a minute or
//! two and costs a little. Run it with
//!
//! ```text
//! cargo build
//! GITCRAFT_E2E_EXE=target/debug/gitcraft.exe GITCRAFT_E2E_ENGINE=claude \
//!     cargo test --lib -- --ignored resolver::e2e_tests --nocapture
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::agent::{self, Activity, Engine};
use super::process::Running;
use super::report::{Progress, Report, StepStatus};
use super::workspace::{FileResult, Workspace};
use crate::ai::run::CommandLine;
use crate::git::fixture::Repo;
use crate::mcp::link;
use crate::mcp::wire::{Outcome, Reply, WriteOp};

/// How long the agent may take.
const WITHIN: Duration = Duration::from_secs(600);

fn conflicted() -> Result<Repo, String> {
    let repo = Repo::new("resolve-e2e")?;
    let base = "fn main() {\n    let names = [\"ada\"];\n    for n in names {\n        println!(\"hi {n}\");\n    }\n}\n";
    repo.commit("base", &[("src/main.rs", Some(base))])?;
    repo.git(&["checkout", "-q", "-b", "greeting"])?;
    repo.commit(
        "Greet politely",
        &[("src/main.rs", Some(&base.replace("hi {n}", "hello, {n}!")))],
    )?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit(
        "Add grace",
        &[(
            "src/main.rs",
            Some(
                &base
                    .replace("[\"ada\"]", "[\"ada\", \"grace\"]")
                    .replace("hi {n}", "hey {n}"),
            ),
        )],
    )?;
    assert!(
        repo.git(&["merge", "-q", "greeting"]).is_err(),
        "it conflicts"
    );
    Ok(repo)
}

#[test]
#[ignore = "runs a real agent: see the module's documentation"]
fn a_real_agent_resolves_a_merge_and_reports_each_step() -> Result<(), String> {
    let exe = PathBuf::from(std::env::var("GITCRAFT_E2E_EXE").map_err(|_| "set GITCRAFT_E2E_EXE")?);
    let exe = std::fs::canonicalize(&exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    let engine = match std::env::var("GITCRAFT_E2E_ENGINE").as_deref() {
        Ok("codex") => Engine::Codex,
        _ => Engine::Claude,
    };
    let repo = conflicted()?;
    let files = vec!["src/main.rs".to_owned()];
    let ws = Workspace::prepare(repo.path(), &files)?;

    // The window: takes the reports of run `e2e`.
    let link_dir = std::env::temp_dir().join(format!("gitcraft-e2e-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&link_dir);
    let reports = Arc::new(Mutex::new(Vec::<(Instant, Report)>::new()));
    let got = Arc::clone(&reports);
    let listener = link::listen(&link_dir, move |incoming| {
        if let WriteOp::Report { run, report } = incoming.op {
            assert_eq!(run, "e2e");
            println!("report: {report:?}");
            got.lock()
                .map(|mut g| g.push((Instant::now(), report)))
                .ok();
            let _ = incoming.reply.send(Reply::new(Outcome::Done, "noted"));
        } else {
            let _ = incoming.reply.send(Reply::new(Outcome::NotOpen, ""));
        }
    })?;

    // As the window starts it, but with this test's window to report to.
    let mcp_file = std::env::temp_dir().join(format!("gitcraft-e2e-{}.json", std::process::id()));
    let mut config = agent::claude_mcp_config(&exe, ws.dir(), "e2e");
    config["mcpServers"]["gitcraft"]["env"] =
        serde_json::json!({ "GITCRAFT_MCP_DIR": link_dir.display().to_string() });
    std::fs::write(&mcp_file, config.to_string()).map_err(|e| e.to_string())?;
    let mut command = agent::command(engine, &exe, ws.dir(), "e2e", &mcp_file)?;
    if let (Engine::Codex, CommandLine::Words(words)) = (engine, &mut command) {
        let at = words.len() - 1;
        words.insert(at, "-c".to_owned());
        words.insert(
            at + 1,
            format!(
                "mcp_servers.gitcraft.env={{GITCRAFT_MCP_DIR='{}'}}",
                link_dir.display()
            ),
        );
    }
    let started = Instant::now();
    let running = Running::start(
        engine,
        &command,
        ws.dir(),
        agent::prompt(&ws, engine),
        || {},
    )?;
    let mut shown = 0;
    let exit = loop {
        let (exit, new) = running.with_feed(|f| {
            (
                f.exit.clone(),
                f.activities.iter().skip(shown).cloned().collect::<Vec<_>>(),
            )
        });
        for a in &new {
            println!("[{:>5.1}s] {a:?}", started.elapsed().as_secs_f32());
        }
        shown += new.len();
        if let Some(exit) = exit {
            break exit;
        }
        if started.elapsed() > WITHIN {
            running.stop();
            return Err("the agent took too long".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    drop(listener);
    let _ = std::fs::remove_file(&mcp_file);
    let _ = std::fs::remove_dir_all(&link_dir);
    println!("exit: {exit:?} after {:?}", started.elapsed());

    let reports = reports.lock().map(|r| r.clone()).unwrap_or_default();
    let mut progress = Progress::default();
    for (at, r) in reports.iter().cloned() {
        progress.apply(r, at);
    }
    let result = ws.check(repo.path(), &[]);
    let text = std::fs::read_to_string(ws.dir().join("src/main.rs")).unwrap_or_default();
    println!("resolved:\n{text}");
    let ended = running.with_feed(|f| {
        f.activities
            .iter()
            .any(|a| matches!(a, Activity::Ended { .. }))
    });
    ws.remove()?;

    assert_eq!(exit.code, Some(0), "{exit:?}");
    assert!(ended, "its last event was read");
    assert!(
        matches!(reports.first(), Some((_, Report::Plan { .. }))),
        "it planned first: {reports:?}"
    );
    assert!(
        progress.steps.iter().any(|s| s.status == StepStatus::Done),
        "a step was done: {:?}",
        progress.steps
    );
    assert!(
        progress.finished.as_ref().is_some_and(|f| f.ok),
        "it finished: {reports:?}"
    );
    assert_eq!(
        result,
        vec![("src/main.rs".to_owned(), FileResult::Resolved)]
    );
    assert!(
        text.contains("grace") && text.contains("hello"),
        "both sides kept:\n{text}"
    );
    Ok(())
}
