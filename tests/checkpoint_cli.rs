//! `gitcraft checkpoint` as an agent's hook runs it: the real binary, the hook's JSON on stdin
//! (Claude Code) or as an argument (Codex), no window.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn hook(args: &[&str], stdin: &str) -> Result<Output, String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gitcraft"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(stdin.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    child.wait_with_output().map_err(|e| e.to_string())
}

#[test]
fn the_binary_saves_a_turn_for_claude_and_codex_hooks() -> Result<(), String> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-cli-checkpoint-{} ä", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        git(&dir, args)?;
    }
    let cwd = serde_json::to_string(&dir.display().to_string()).map_err(|e| e.to_string())?;
    let prompt = hook(
        &["checkpoint"],
        &format!(
            r#"{{"session_id":"abc","hook_event_name":"UserPromptSubmit","cwd":{cwd},"prompt":"Write notes"}}"#
        ),
    )?;
    std::fs::write(dir.join("notes.md"), "n\n").map_err(|e| e.to_string())?;
    let stop = hook(
        &["checkpoint"],
        &format!(r#"{{"session_id":"abc","hook_event_name":"Stop","cwd":{cwd}}}"#),
    )?;
    std::fs::write(dir.join("more.md"), "m\n").map_err(|e| e.to_string())?;
    let notify = format!(
        r#"{{"type":"agent-turn-complete","thread-id":"t1","cwd":{cwd},"input-messages":["Add more"]}}"#
    );
    let codex = hook(&["checkpoint", "--codex", &notify], "")?;
    // Garbage in: said on stderr, never a failure for the agent.
    let bad = hook(&["checkpoint"], "not json")?;
    let claude_turn = git(
        &dir,
        &["log", "-1", "--format=%s", "refs/gitcraft/agent/abc/1"],
    );
    let codex_turn = git(
        &dir,
        &["log", "-1", "--format=%s", "refs/gitcraft/agent/t1/1"],
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(prompt.status.success() && stop.status.success() && codex.status.success());
    assert_eq!(claude_turn?.trim(), "Write notes");
    assert_eq!(codex_turn?.trim(), "Add more");
    assert!(bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("gitcraft checkpoint:"));
    Ok(())
}
