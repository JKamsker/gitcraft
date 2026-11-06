//! Agent checkpoints on real repositories: turns saved as the hooks save them, by gitcraft and
//! by the script for other machines alike, labelled with their prompts, listed per worktree.

use std::path::{Path, PathBuf};

use super::checkpoint::{self, HookEvent, REFS};
use crate::git;

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}

fn repo(name: &str) -> Repo {
    let root = std::env::temp_dir().join(format!(
        "gitcraft-checkpoint-{name}-{} ü",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("work tree");
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join("a.txt"), "a\n");
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-qm", "base"]).is_ok());
    Repo(dir)
}

fn top(dir: &Path) -> String {
    git::run(Some(dir), &["rev-parse", "--show-toplevel"])
        .unwrap_or_default()
        .trim_end()
        .to_owned()
}

fn event(kind: &str, session: &str, dir: &Path, prompt: &str) -> HookEvent {
    HookEvent {
        event: kind.to_owned(),
        session: session.to_owned(),
        cwd: dir.display().to_string(),
        prompt: prompt.to_owned(),
    }
}

#[test]
fn each_turn_is_saved_with_its_prompt_and_an_unchanged_one_is_not() -> Result<(), String> {
    let r = repo("turns");
    let dir = &r.0;
    let head = git::head(dir);
    let status_before = git::run(Some(dir), &["status", "--porcelain"])?;
    // The prompt arrives first, the turn after.
    checkpoint::handle(
        &event("UserPromptSubmit", "s 1", dir, "Add b ü\nand more"),
        dir,
    )?;
    std::fs::write(dir.join("b.txt"), "b\n").map_err(|e| e.to_string())?;
    let first = checkpoint::handle(&event("Stop", "s 1", dir, ""), dir)?;
    assert!(first.is_some());
    // Nothing changed: no turn.
    assert!(checkpoint::handle(&event("Stop", "s 1", dir, ""), dir)?.is_none());
    std::fs::write(dir.join("a.txt"), "A\n").map_err(|e| e.to_string())?;
    checkpoint::handle(&event("Stop", "s 1", dir, ""), dir)?;
    let turns = checkpoint::turns(dir, &top(dir))?;
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert_eq!((turns[0].number, turns[0].label.as_str()), (1, "Add b ü"));
    assert_eq!(turns[0].prompt, "Add b ü\nand more");
    assert_eq!(turns[0].parent, head);
    assert_eq!((turns[1].number, turns[1].label.as_str()), (2, "Turn 2"));
    assert_eq!(turns[1].parent, turns[0].commit);
    assert!(git::run(Some(dir), &["rev-parse", &format!("{REFS}/s-1/2")]).is_ok());
    // The turn's diff is what the agent did in it.
    let diff = git::run(
        Some(dir),
        &["diff", "--name-only", &turns[1].parent, &turns[1].commit],
    )?;
    assert_eq!(diff, "a.txt\n");
    // Nothing of the user's moved: HEAD, the index, the graph's history.
    assert_eq!(git::head(dir), head);
    let status = git::run(Some(dir), &["status", "--porcelain"])?;
    assert!(status_before.is_empty());
    assert_eq!(status, " M a.txt\n?? b.txt\n");
    let log = git::log(dir, None, 0, 10)?;
    assert_eq!(log.len(), 1, "turns aren't history");
    // Another worktree's turns aren't this one's.
    assert!(checkpoint::turns(dir, "/elsewhere")?.is_empty());
    checkpoint::forget(dir, "s-1")?;
    assert!(checkpoint::turns(dir, &top(dir))?.is_empty());
    Ok(())
}

#[test]
fn a_hook_input_that_is_not_json_says_so() {
    assert!(checkpoint::parse_event("", false).is_err());
}

/// The script other machines run saves the same turns as gitcraft does.
#[cfg(unix)]
#[test]
fn the_script_for_other_machines_saves_what_gitcraft_saves() -> Result<(), String> {
    let r = repo("parity");
    let dir = &r.0;
    let script = include_str!("checkpoint.sh");
    let run = |args: &[&str], stdin: &str| -> Result<(), String> {
        use std::io::Write;
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .arg("sh")
            .args(args)
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(stdin.as_bytes());
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(())
    };
    let cwd = serde_json::to_string(&dir.display().to_string()).map_err(|e| e.to_string())?;
    run(
        &[],
        &format!(
            r#"{{"session_id":"sh 1","hook_event_name":"UserPromptSubmit","cwd":{cwd},"prompt":"Fix \"it\" now"}}"#
        ),
    )?;
    std::fs::write(dir.join("c.txt"), "c\n").map_err(|e| e.to_string())?;
    run(
        &[],
        &format!(r#"{{"session_id":"sh 1","hook_event_name":"Stop","cwd":{cwd}}}"#),
    )?;
    // Unchanged: nothing.
    run(
        &[],
        &format!(r#"{{"session_id":"sh 1","hook_event_name":"Stop","cwd":{cwd}}}"#),
    )?;
    std::fs::write(dir.join("d.txt"), "d\n").map_err(|e| e.to_string())?;
    run(
        &[
            "--codex",
            &format!(
                r#"{{"type":"agent-turn-complete","thread-id":"cx","cwd":{cwd},"input-messages":["one","Then [this]"]}}"#
            ),
        ],
        "",
    )?;
    let by_script = checkpoint::turns(dir, &top(dir))?;
    // gitcraft, the same turn on a twin repository state.
    let r2 = repo("parity-twin");
    let twin = &r2.0;
    std::fs::write(twin.join("c.txt"), "c\n").map_err(|e| e.to_string())?;
    checkpoint::handle(&event("Stop", "sh 1", twin, "Fix \"it\" now"), twin)?;
    let by_gitcraft = checkpoint::turns(twin, &top(twin))?;
    let script_turn = by_script.iter().find(|t| t.session == "sh-1");
    assert_eq!(
        script_turn.map(|t| t.label.as_str()),
        Some("Fix \"it\" now"),
        "{by_script:?}"
    );
    assert_eq!(script_turn.map(|t| t.number), Some(1));
    let tree = |d: &Path, c: &str| git::run(Some(d), &["rev-parse", &format!("{c}^{{tree}}")]);
    assert_eq!(
        tree(
            dir,
            &script_turn.map(|t| t.commit.clone()).unwrap_or_default()
        )?,
        tree(twin, &by_gitcraft[0].commit)?,
        "the same tree"
    );
    let body = |d: &Path, c: &str| git::run(Some(d), &["log", "-1", "--format=%B", c]);
    let script_msg = body(
        dir,
        &script_turn.map(|t| t.commit.clone()).unwrap_or_default(),
    )?;
    let gitcraft_msg = body(twin, &by_gitcraft[0].commit)?;
    assert_eq!(
        script_msg.replace(&top(dir), "<top>"),
        gitcraft_msg.replace(&top(twin), "<top>")
    );
    let codex = by_script.iter().find(|t| t.session == "cx");
    assert_eq!(
        codex.map(|t| t.label.as_str()),
        Some("Then [this]"),
        "{by_script:?}"
    );
    Ok(())
}

/// A hook run from a subfolder (an agent started in a monorepo's package) saves the turn too.
#[test]
fn a_turn_is_saved_from_a_subfolder_and_keeps_a_prompt_that_quotes_trailers() -> Result<(), String>
{
    let r = repo("subfolder");
    let dir = &r.0;
    let sub = dir.join("pkg ü");
    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    std::fs::write(sub.join("x.txt"), "x\n").map_err(|e| e.to_string())?;
    let prompt = "Explain\n\nGitcraft-Session: fake\nGitcraft-Worktree: /nowhere";
    let saved = checkpoint::handle(&event("Stop", "s", &sub, prompt), &sub)?;
    assert!(saved.is_some());
    let turns = checkpoint::turns(dir, &top(dir))?;
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert_eq!(turns[0].prompt, prompt);
    Ok(())
}

/// The script reads JSON's escapes as JSON means them: `C:\\new` is a backslash and `new`, an
/// escaped quote is a quote, blank lines before a prompt don't name the turn.
#[cfg(unix)]
#[test]
fn the_script_reads_escapes_and_blank_lines_as_gitcraft_does() -> Result<(), String> {
    let r = repo("escapes");
    let dir = &r.0;
    let script = include_str!("checkpoint.sh");
    let run = |args: &[&str], stdin: &str| -> Result<(), String> {
        use std::io::Write;
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .arg("sh")
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(stdin.as_bytes());
        }
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    };
    let cwd = serde_json::to_string(&dir.display().to_string()).map_err(|e| e.to_string())?;
    let prompt = serde_json::to_string("\n\nopen C:\\new\\temp \"now\"\r\nthen\tgo")
        .map_err(|e| e.to_string())?;
    run(
        &[],
        &format!(
            r#"{{"session_id":"e","hook_event_name":"UserPromptSubmit","cwd":{cwd},"prompt":{prompt}}}"#
        ),
    )?;
    std::fs::write(dir.join("e.txt"), "e\n").map_err(|e| e.to_string())?;
    run(
        &[],
        &format!(r#"{{"session_id":"e","hook_event_name":"Stop","cwd":{cwd}}}"#),
    )?;
    std::fs::write(dir.join("f.txt"), "f\n").map_err(|e| e.to_string())?;
    let notify = format!(
        r#"{{"type":"agent-turn-complete","thread-id":"q","cwd":{cwd},"input-messages":["a","say \"hi\" now"]}}"#
    );
    run(&["--codex", &notify], "")?;
    let turns = checkpoint::turns(dir, &top(dir))?;
    let by = |s: &str| {
        turns
            .iter()
            .find(|t| t.session == s)
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(by("e").label, "open C:\\new\\temp \"now\"", "{turns:?}");
    assert_eq!(
        by("e").prompt,
        "open C:\\new\\temp \"now\"\nthen\tgo",
        "{turns:?}"
    );
    assert_eq!(by("q").label, "say \"hi\" now", "{turns:?}");
    Ok(())
}
