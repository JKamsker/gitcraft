use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Source, Who, agents, attribute, branch, gitai, load, trace};

/// The example of git-ai's standard 3.0.0, with a legacy prompt and a quoted path added.
const NOTE: &str = r#"src/main.rs
  s_c9883b05a2487d::t_9f8e7d6c5b4a32 1-10,15-20
  s_c9883b05a2487d::t_a1b2c3d4e5f678 25,30-35
  h_31dce776f88375 42-50
"src/my lib.rs"
  0123456789abcdef 1-5
---
{
  "schema_version": "authorship/3.0.0",
  "git_ai_version": "1.4.5",
  "base_commit_sha": "7734793b756b3921c88db5375a8c156e9532447b",
  "prompts": {"0123456789abcdef": {"agent_id": {"tool": "claude", "id": "x", "model": "m"}, "total_additions": 5, "total_deletions": 0, "accepted_lines": 5, "overriden_lines": 0}},
  "humans": {"h_31dce776f88375": {"author": "Developer <dev@example.com>"}},
  "sessions": {"s_c9883b05a2487d": {"agent_id": {"tool": "cursor", "id": "6ef2", "model": "claude-sonnet"}, "human_author": "dev@example.com"}}
}"#;

#[test]
fn git_ai_notes_line_by_line() {
    let Some(note) = gitai::parse(NOTE) else {
        panic_free_fail("the note parses");
        return;
    };
    let main = &note.files["src/main.rs"];
    assert_eq!(main.len(), 5);
    assert!(main.contains(&(1, 10, Who::Agent)));
    assert!(main.contains(&(25, 25, Who::Agent)));
    assert!(main.contains(&(42, 50, Who::Human)));
    assert_eq!(note.files["src/my lib.rs"], vec![(1, 5, Who::Agent)]);
    let mut tools = note.tools.clone();
    tools.sort();
    assert_eq!(tools, vec!["claude", "cursor"]);
    let a = attribute("Dev", "dev@example.com", "msg", Some(NOTE), None);
    let Some(a) = a else {
        panic_free_fail("attributed");
        return;
    };
    assert_eq!(a.who, Who::Mixed);
    assert_eq!(a.source, Source::GitAi);
    assert_eq!(a.line("src/main.rs", 5), Who::Agent);
    assert_eq!(a.line("src/main.rs", 45), Who::Human);
    // No key covers it: not known, never guessed.
    assert_eq!(a.line("src/main.rs", 12), Who::Unknown);
    assert_eq!(a.line("other.rs", 1), Who::Unknown);
    assert!(gitai::parse("not a note").is_none());
    assert!(gitai::parse("x\n---\n{\"schema_version\": \"other/1\"}").is_none());
}

/// Fails a test without `panic!` (denied by the lints).
pub(crate) fn panic_free_fail(what: &str) {
    assert!(what.is_empty(), "{what}");
}

#[test]
fn agent_trace_records() {
    let record = r#"{"version":"0.1.0","id":"u","timestamp":"t","vcs":{"type":"git","revision":"abc"},"tool":{"name":"cursor","version":"2.4.0"},"files":[{"path":"src/utils/parser.ts","conversations":[{"url":"x","contributor":{"type":"ai","model_id":"m"},"ranges":[{"start_line":1,"end_line":20},{"start_line":30,"end_line":31,"contributor":{"type":"human"}}]}]}]}"#;
    let (lines, tools) = trace::parse(record).unwrap_or_default();
    assert_eq!(tools, vec!["cursor"]);
    assert_eq!(
        lines["src/utils/parser.ts"],
        vec![(1, 20, Who::Agent), (30, 31, Who::Human)]
    );
    // JSON lines, and a list.
    let jsonl = format!("{record}\n{record}\n");
    assert_eq!(
        trace::parse(&jsonl).map(|(l, _)| l["src/utils/parser.ts"].len()),
        Some(4)
    );
    assert!(trace::parse(&format!("[{record}]")).is_some());
    assert!(trace::parse("garbage").is_none());
    let a = attribute("Dev", "d@x", "", None, Some(record));
    assert!(a.is_some_and(|a| a.who == Who::Mixed && a.source == Source::AgentTrace));
}

#[test]
fn authors_and_trailers() {
    let msg = "feat: x\n\nBody.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nClaude-Session: https://x\n";
    assert_eq!(agents::co_authors(msg), vec!["Claude"]);
    let a = attribute("Ana", "ana@x.org", msg, None, None);
    assert!(a.is_some_and(|a| a.who == Who::Mixed
        && a.source == Source::Trailer
        && a.agents == ["Claude"]));
    assert_eq!(
        agents::co_authors("x\n\nco-authored-by: Cursor Agent <cursoragent@cursor.com>"),
        vec!["Cursor"]
    );
    assert!(agents::co_authors("x\n\nCo-authored-by: Bob <bob@x.org>").is_empty());
    // A bot account as the author: an agent's commit.
    let bot = attribute(
        "Copilot",
        "198982749+Copilot@users.noreply.github.com",
        "fix",
        None,
        None,
    );
    assert!(bot.is_none(), "a noreply address is no bot by itself");
    let bot = attribute(
        "copilot-swe-agent[bot]",
        "198982749+Copilot@users.noreply.github.com",
        "fix",
        None,
        None,
    );
    assert!(bot.is_some_and(|a| a.who == Who::Agent && a.source == Source::Author));
    // A person called Claude is a person.
    assert!(attribute("Claude Monet", "claude@monet.fr", "paint", None, None).is_none());
    // Nothing recorded: nothing said.
    assert!(attribute("Ana", "ana@x.org", "plain", None, None).is_none());
}

#[test]
fn a_patch_read_for_its_lines() {
    let patch = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -3,2 +3,3 @@\n-x\n-y\n+a\n+b\n+c\n@@ -10 +11,0 @@\n-z\ndiff --git a/new file.rs b/new file.rs\n--- /dev/null\n+++ b/new file.rs\n@@ -0,0 +1 @@\n+n\n";
    let (added, removed) = branch::changes_for_test(patch);
    assert_eq!(
        added,
        vec![
            ("a.rs".into(), 3),
            ("a.rs".into(), 4),
            ("a.rs".into(), 5),
            ("new file.rs".into(), 1)
        ]
    );
    assert_eq!(removed, vec![("a.rs".into(), 3, 2), ("a.rs".into(), 10, 1)]);
}

#[test]
fn the_branch_headline() {
    let f = branch::Figures {
        commits: 3,
        agent: 71,
        human: 29,
        edited: 6,
        ..Default::default()
    };
    assert_eq!(
        f.headline("agent/x"),
        "agent/x: 71% agent-written (71 of 100 lines added in 3 commits), 8% of that since edited by a person"
    );
    assert_eq!(
        branch::Figures::default().headline("x"),
        "x adds no lines since it left the base"
    );
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

pub(crate) fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-authors-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Ana"],
        &["config", "user.email", "ana@x.org"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(&dir, args)?;
    }
    std::fs::write(dir.join("a.rs"), "one\n").map_err(|e| e.to_string())?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "base"])?;
    Ok(dir)
}

/// A commit of `text` to `file` with `message`, its hash.
pub(crate) fn commit(dir: &Path, file: &str, text: &str, message: &str) -> Result<String, String> {
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", message])?;
    Ok(git(dir, &["rev-parse", "HEAD"])?.trim().to_owned())
}

#[test]
fn a_real_repository_with_notes_and_trailers() -> Result<(), String> {
    let dir = repo("real")?;
    git(&dir, &["checkout", "-q", "-b", "agent/work"])?;
    // An agent writes four lines (git-ai note), then a person rewrites one of them.
    let by_agent = commit(&dir, "ünï code.rs", "a1\na2\na3\na4\n", "agent work")?;
    let note = "\"ünï code.rs\"\n  0123456789abcdef 1-4\n---\n{\"schema_version\":\"authorship/3.0.0\",\"base_commit_sha\":\"x\",\"prompts\":{\"0123456789abcdef\":{\"agent_id\":{\"tool\":\"claude\",\"id\":\"i\",\"model\":\"m\"}}}}\n";
    git(&dir, &["notes", "--ref=ai", "add", "-m", note, &by_agent])?;
    let by_person = commit(&dir, "ünï code.rs", "a1\nA2 by hand\na3\na4\n", "tidy")?;
    // Co-written per its trailer; and one with an Agent Trace note.
    let co = commit(
        &dir,
        "b.rs",
        "b\n",
        "feat: b\n\nCo-authored-by: Codex <codex@openai.com>",
    )?;
    let traced = commit(&dir, "c.rs", "c1\nc2\n", "c")?;
    let record = format!(
        "{{\"version\":\"0.1.0\",\"vcs\":{{\"type\":\"git\",\"revision\":\"{traced}\"}},\"tool\":{{\"name\":\"cursor\"}},\"files\":[{{\"path\":\"c.rs\",\"conversations\":[{{\"contributor\":{{\"type\":\"ai\"}},\"ranges\":[{{\"start_line\":1,\"end_line\":2}}]}}]}}]}}"
    );
    git(
        &dir,
        &["notes", "--ref=agent-trace", "add", "-m", &record, &traced],
    )?;

    let all = load::all(&dir)?;
    assert_eq!(all.get(&by_agent).map(|a| a.who), Some(Who::Agent));
    assert_eq!(
        all.get(&by_agent).map(|a| a.line("ünï code.rs", 2)),
        Some(Who::Agent)
    );
    assert!(!all.contains_key(&by_person));
    assert_eq!(all.get(&co).map(|a| a.who), Some(Who::Mixed));
    assert_eq!(all.get(&traced).map(|a| a.source), Some(Source::AgentTrace));
    assert_eq!(
        load::one(&dir, &by_agent)?.map(|a| a.source),
        Some(Source::GitAi)
    );
    assert_eq!(load::one(&dir, &traced)?.map(|a| a.who), Some(Who::Agent));
    assert!(load::one(&dir, &by_person)?.is_none());
    assert!(load::one(&dir, "--x").is_err());

    let figures = branch::figures(&dir, "refs/heads/agent/work", "refs/heads/main", &all)?;
    // 4 agent lines + 2 traced ones; 1 line by hand and 1 co-written.
    assert_eq!(figures.agent, 6, "{figures:?}");
    assert_eq!(figures.unknown, 1);
    assert_eq!(figures.mixed, 1);
    assert_eq!(figures.edited, 1);
    assert_eq!(figures.commits, 4);
    // Read again without the map: the same.
    assert_eq!(
        branch::figures(
            &dir,
            "refs/heads/agent/work",
            "refs/heads/main",
            &HashMap::new()
        )?,
        figures
    );
    // A repository with no notes at all.
    let plain = repo("plain")?;
    assert!(load::all(&plain)?.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&plain);
    Ok(())
}
