use std::path::Path;

use super::*;

const DIR: &str = "/tmp/gitcraft-scratch/abc/resolve-host-1-0";

#[test]
fn claude_runs_headless_with_only_gitcraft_and_safe_tools() -> Result<(), String> {
    let line = command(
        Engine::Claude,
        Path::new("/bin/gitcraft"),
        Path::new(DIR),
        "r1",
        Path::new("/tmp/mcp.json"),
    )?;
    let CommandLine::Words(words) = &line else {
        return Err("not words".into());
    };
    let joined = words.join(" ");
    assert!(
        joined.starts_with("claude -p --output-format stream-json"),
        "{joined}"
    );
    assert!(
        joined.contains("--strict-mcp-config --mcp-config /tmp/mcp.json"),
        "{joined}"
    );
    assert!(
        joined.ends_with(&format!("--allowedTools {CLAUDE_TOOLS}")),
        "{joined}"
    );
    // On Windows `cmd` keeps the tool list one word.
    assert!(line.cmd().contains(&format!("\"{CLAUDE_TOOLS}\"")));
    // Nothing that commits, stages or pushes is let through.
    for refused in [
        "git add",
        "git commit",
        "git push",
        "git merge:",
        "git checkout",
        "Bash,",
        "Bash(*",
    ] {
        assert!(!CLAUDE_TOOLS.contains(refused), "{refused}");
    }
    let config = claude_mcp_config(Path::new("/bin/gitcraft"), Path::new(DIR), "r1");
    assert_eq!(config["mcpServers"]["gitcraft"]["command"], "/bin/gitcraft");
    assert_eq!(
        config["mcpServers"]["gitcraft"]["args"],
        json!(["mcp", "--repo", DIR, "--run", "r1"])
    );
    Ok(())
}

#[test]
fn codex_is_given_gitcraft_on_its_command_line() -> Result<(), String> {
    let line = command(
        Engine::Codex,
        Path::new("/bin/gitcraft"),
        Path::new(DIR),
        "r1",
        Path::new("/unused"),
    )?;
    let CommandLine::Words(words) = &line else {
        return Err("not words".into());
    };
    assert_eq!(words[..3], ["codex", "exec", "--json"]);
    assert!(words.contains(&"mcp_servers.gitcraft.command='/bin/gitcraft'".to_owned()));
    assert!(words.contains(&format!(
        "mcp_servers.gitcraft.args=['mcp','--repo','{DIR}','--run','r1']"
    )));
    assert_eq!(
        words.last().map(String::as_str),
        Some("-"),
        "the prompt comes on its input"
    );
    // A path TOML can't hold is refused, not mangled.
    assert!(
        command(
            Engine::Codex,
            Path::new("/o'brien/gitcraft"),
            Path::new(DIR),
            "r",
            Path::new("/u")
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn claude_events_read_as_what_it_does() {
    let tool = format!(
        r#"{{"type":"assistant","message":{{"content":[
            {{"type":"text","text":"\nLooking at both sides.\nMore."}},
            {{"type":"tool_use","name":"Edit","input":{{"file_path":"{DIR}/src/a.rs"}}}},
            {{"type":"tool_use","name":"mcp__gitcraft__resolve_step","input":{{}}}},
            {{"type":"tool_use","name":"mcp__gitcraft__blame","input":{{}}}},
            {{"type":"tool_use","name":"Bash","input":{{"command":"git log -3"}}}}
        ]}}}}"#
    )
    .replace('\n', "");
    assert_eq!(
        activities(Engine::Claude, &tool, DIR),
        vec![
            Activity::Said("Looking at both sides.".into()),
            Activity::Tool("Editing src/a.rs".into()),
            Activity::Tool("Asking gitcraft: blame".into()),
            Activity::Tool("Running git log -3".into()),
        ]
    );
    let end = r#"{"type":"result","subtype":"success","is_error":false,"result":"All done","total_cost_usd":0.25}"#;
    assert_eq!(
        activities(Engine::Claude, end, DIR),
        vec![Activity::Ended {
            ok: true,
            text: "All done".into(),
            cost_usd: Some(0.25)
        }]
    );
    assert!(activities(Engine::Claude, "not json", DIR).is_empty());
    assert!(activities(Engine::Claude, r#"{"type":"user"}"#, DIR).is_empty());
}

#[test]
fn codex_events_read_as_what_it_does() {
    let ran = r#"{"type":"item.started","item":{"type":"command_execution","command":"git diff"}}"#;
    assert_eq!(
        activities(Engine::Codex, ran, DIR),
        vec![Activity::Tool("Running git diff".into())]
    );
    let changed = format!(
        r#"{{"type":"item.completed","item":{{"type":"file_change","changes":[{{"path":"{DIR}/b.txt"}}]}}}}"#
    );
    assert_eq!(
        activities(Engine::Codex, &changed, DIR),
        vec![Activity::Tool("Editing b.txt".into())]
    );
    let report = r#"{"type":"item.started","item":{"type":"mcp_tool_call","tool":"resolve_plan"}}"#;
    assert!(activities(Engine::Codex, report, DIR).is_empty());
    let failed = r#"{"type":"turn.failed","error":{"message":"quota"}}"#;
    assert!(matches!(
        &activities(Engine::Codex, failed, DIR)[..],
        [Activity::Ended { ok: false, text, .. }] if text == "quota"
    ));
}

#[test]
fn the_prompt_names_the_files_the_sides_and_the_tools() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("resolve-prompt")?;
    repo.commit("base", &[("a.txt", Some("a\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side", &[("a.txt", Some("side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("main\n"))])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    let ws = Workspace::prepare(repo.path(), &["a.txt".to_owned()])?;
    let text = prompt(&ws);
    assert!(text.contains("a merge of side ("), "{text}");
    assert!(text.contains("into main ("), "{text}");
    assert!(text.contains("- a.txt (1 conflict)"), "{text}");
    for tool in [
        "resolve_plan",
        "resolve_step",
        "resolve_finish",
        "estimate_minutes",
    ] {
        assert!(text.contains(tool), "{tool}");
    }
    assert!(text.contains("Don't stage, commit"), "{text}");
    ws.remove()
}

#[test]
fn the_shell_codex_wraps_commands_in_is_left_out() {
    let pwsh = r#""C:\Program Files\PowerShell\pwsh.exe" -Command 'git log -5 MERGE_HEAD'"#;
    assert_eq!(bare_command(pwsh), "git log -5 MERGE_HEAD");
    assert_eq!(
        bare_command("/bin/bash -lc 'git diff --check'"),
        "git diff --check"
    );
    assert_eq!(
        bare_command("git diff -c x"),
        "git diff -c x",
        "not a shell: kept"
    );
    assert_eq!(bare_command("git status"), "git status");
}
