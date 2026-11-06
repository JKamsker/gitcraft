//! Installing the agent hooks that save each turn as a checkpoint, in a worktree: Claude Code's
//! `UserPromptSubmit` (the prompt) and `Stop` (the turn) in `.claude/settings.local.json`, and
//! Codex's `notify` in `.codex/config.toml`. Both files are kept out of git through the
//! repository's `info/exclude`. On another machine (WSL, SSH), where gitcraft isn't, the hooks
//! run a script gitcraft writes into the repository's git folder instead.

use std::path::Path;

use serde_json::{Value, json};

/// The script for other machines, as written there.
const SCRIPT: &str = include_str!("checkpoint.sh");
const CLAUDE_FILE: &str = ".claude/settings.local.json";
const CODEX_FILE: &str = ".codex/config.toml";

/// What runs a checkpoint: (Claude's hook command, Codex's `notify` words).
fn program(worktree: &Path) -> Result<(String, Vec<String>), String> {
    if crate::remote::is_remote(worktree) {
        let script = super::repo_file(worktree, "checkpoint.sh")?;
        if let Some(dir) = script.parent() {
            crate::remote::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        crate::remote::write(&script, SCRIPT.as_bytes())
            .map_err(|e| format!("could not write the checkpoint script: {e}"))?;
        let there = crate::remote::git_path(&script);
        return Ok((
            format!("sh {}", crate::remote::sh_quote(&there)),
            vec!["sh".to_owned(), there, "--codex".to_owned()],
        ));
    }
    let exe = std::env::current_exe()
        .map_err(|e| format!("could not tell where gitcraft is: {e}"))?
        .display()
        .to_string();
    // Claude Code runs hooks with a POSIX shell (Git Bash on Windows): `/`, quoted.
    let exe = if cfg!(windows) {
        exe.replace('\\', "/")
    } else {
        exe
    };
    Ok((
        format!("{} checkpoint", crate::remote::sh_quote(&exe)),
        vec![exe, "checkpoint".to_owned(), "--codex".to_owned()],
    ))
}

/// Whether a hook command is one of ours.
fn ours(command: &str) -> bool {
    command.contains("checkpoint") && command.contains("gitcraft")
}

/// Claude Code's settings with our two hooks in, whatever else they hold; an older command of
/// ours is replaced.
pub(crate) fn with_claude_hooks(settings: &str, command: &str) -> Result<String, String> {
    let mut value: Value = if settings.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(settings).map_err(|e| format!("{CLAUDE_FILE} isn't JSON: {e}"))?
    };
    let root = value
        .as_object_mut()
        .ok_or_else(|| format!("{CLAUDE_FILE} isn't a JSON object"))?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("\"hooks\" in {CLAUDE_FILE} isn't an object"))?;
    for event in ["UserPromptSubmit", "Stop"] {
        let list = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("\"hooks.{event}\" in {CLAUDE_FILE} isn't a list"))?;
        // Ours out (an older gitcraft's), then in once.
        for group in list.iter_mut() {
            if let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                inner.retain(|h| !h.get("command").and_then(Value::as_str).is_some_and(ours));
            }
        }
        list.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
        list.push(json!({ "hooks": [{ "type": "command", "command": command }] }));
    }
    serde_json::to_string_pretty(&value)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

/// Codex's config with `notify` running ours; `Err` saying why when it runs another program.
pub(crate) fn with_codex_notify(config: &str, words: &[String]) -> Result<String, String> {
    let quoted: Vec<String> = words
        .iter()
        .map(|w| serde_json::to_string(w).unwrap_or_default())
        .collect();
    let line = format!("notify = [{}]", quoted.join(", "));
    let mut lines: Vec<String> = config.lines().map(str::to_owned).collect();
    // Only a top-level key: before the first table.
    let top = lines
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .unwrap_or(lines.len());
    match lines[..top]
        .iter()
        .position(|l| l.trim_start().starts_with("notify") && l.contains('='))
    {
        Some(i) if ours(&lines[i]) => lines[i] = line,
        Some(_) => {
            return Err(
                "it runs another notify program already: add gitcraft's (\"checkpoint\", \
                 \"--codex\") to it yourself"
                    .to_owned(),
            );
        }
        None => lines.insert(0, line),
    }
    Ok(lines.join("\n") + "\n")
}

/// Installs the hooks in the worktree at `worktree`: what was done, line by line, for the user.
pub(crate) fn install(worktree: &Path) -> Result<Vec<String>, String> {
    let (command, words) = program(worktree)?;
    let mut said = Vec::new();
    said.push(edit(
        worktree,
        CLAUDE_FILE,
        |text| with_claude_hooks(text, &command),
        "Claude Code",
    )?);
    said.push(
        edit(
            worktree,
            CODEX_FILE,
            |text| with_codex_notify(text, &words),
            "Codex",
        )
        .unwrap_or_else(|e| format!("Codex: {e}")),
    );
    exclude(worktree, &[CLAUDE_FILE, CODEX_FILE])?;
    Ok(said)
}

/// Rewrites `rel` in the worktree with `change`, unless git tracks it (the change would be part
/// of the work); what was done.
fn edit(
    worktree: &Path,
    rel: &str,
    change: impl FnOnce(&str) -> Result<String, String>,
    agent: &str,
) -> Result<String, String> {
    if crate::git::run(Some(worktree), &["ls-files", "--error-unmatch", "--", rel]).is_ok() {
        return Ok(format!(
            "{agent}: {rel} is committed in this repository, so it was left as it is: no hooks."
        ));
    }
    let path = crate::remote::join(worktree, rel);
    let text = match crate::remote::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("could not read {rel}: {e}")),
    };
    let new = change(&text)?;
    if let Some(dir) = path.parent() {
        crate::remote::create_dir_all(dir)
            .map_err(|e| format!("could not make {rel}'s folder: {e}"))?;
    }
    crate::remote::write(&path, new.as_bytes())
        .map_err(|e| format!("could not write {rel}: {e}"))?;
    Ok(format!(
        "{agent}: hooks in {rel}, from the next time it starts in this worktree."
    ))
}

/// Keeps `paths` out of git in every worktree of the repository (`info/exclude`).
fn exclude(worktree: &Path, paths: &[&str]) -> Result<(), String> {
    let out = crate::git::run(Some(worktree), &["rev-parse", "--git-common-dir"])?;
    let common = crate::remote::resolve(worktree, out.trim_end_matches(['\n', '\r']));
    let file = crate::remote::join(&common, "info/exclude");
    let text = crate::remote::read_to_string(&file).unwrap_or_default();
    let mut add = String::new();
    for path in paths {
        let line = format!("/{path}");
        if !text.lines().any(|l| l.trim() == line) {
            add.push_str(&line);
            add.push('\n');
        }
    }
    if add.is_empty() {
        return Ok(());
    }
    let mut new = text;
    if !new.is_empty() && !new.ends_with('\n') {
        new.push('\n');
    }
    new.push_str("# Agent hooks gitcraft installed\n");
    new.push_str(&add);
    if let Some(dir) = file.parent() {
        crate::remote::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::remote::write(&file, new.as_bytes())
        .map_err(|e| format!("could not write info/exclude: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_settings_keep_what_they_had_and_get_our_hooks_once() -> Result<(), String> {
        let before = r#"{"permissions":{"allow":["Bash(ls)"]},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"},{"type":"command","command":"/old/gitcraft checkpoint"}]}]}}"#;
        let once = with_claude_hooks(before, "'/new/gitcraft' checkpoint")?;
        let twice = with_claude_hooks(&once, "'/new/gitcraft' checkpoint")?;
        assert_eq!(once, twice);
        let v: Value = serde_json::from_str(&twice).map_err(|e| e.to_string())?;
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
        let stop = v["hooks"]["Stop"].to_string();
        assert!(
            stop.contains("say done") && !stop.contains("/old/"),
            "{stop}"
        );
        assert!(
            v["hooks"]["UserPromptSubmit"]
                .to_string()
                .contains("/new/gitcraft")
        );
        assert!(with_claude_hooks("[1]", "x").is_err());
        assert!(with_claude_hooks("{oops", "x").is_err());
        Ok(())
    }

    #[test]
    fn codex_gets_notify_at_the_top_unless_it_has_its_own() -> Result<(), String> {
        let words = [
            "/a b/gitcraft".to_owned(),
            "checkpoint".to_owned(),
            "--codex".to_owned(),
        ];
        let fresh = with_codex_notify("model = \"o3\"\n[profiles.x]\nnotify = 1\n", &words)?;
        assert!(
            fresh.starts_with("notify = [\"/a b/gitcraft\", \"checkpoint\", \"--codex\"]\nmodel"),
            "{fresh}"
        );
        assert_eq!(
            with_codex_notify(&fresh, &words)?,
            fresh,
            "replaced, not added twice"
        );
        assert!(with_codex_notify("notify = [\"say\"]\n", &words).is_err());
        Ok(())
    }
}
