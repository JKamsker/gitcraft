//! The fence written into each agent's own configuration in a worktree, so that it is told
//! (and, where the agent can, kept) out of the paths: Claude Code's deny rules, Cursor's
//! `.cursorignore`, and a section of `AGENTS.md` for Codex and the agents that read it.
//!
//! What gitcraft wrote before is replaced, and nothing else of those files is touched. The
//! rules can leak (a script the agent runs reads the file anyway), so the worktrees are
//! checked as well (see [`super::check`]).

use std::path::Path;

use serde_json::{Map, Value};

use super::pattern::Pattern;

/// Where each agent's configuration is, from the worktree's top.
pub const CLAUDE: &str = ".claude/settings.local.json";
pub const CURSOR: &str = ".cursorignore";
pub const AGENTS: &str = "AGENTS.md";

const START: &str = "gitcraft agent fence: start";
const END: &str = "gitcraft agent fence: end";

/// The Claude Code rules that keep it from reading or changing what `p` fences: the path
/// itself and everything under it, anywhere in the tree unless the pattern is anchored.
fn claude_rules(p: &Pattern) -> Vec<String> {
    let body = p.text.trim().trim_start_matches('/').trim_end_matches('/');
    let glob = if p.anchored() {
        body.to_owned()
    } else {
        format!("**/{body}")
    };
    let globs = [glob.clone(), format!("{glob}/**")];
    ["Read", "Edit"]
        .iter()
        .flat_map(|tool| globs.iter().map(move |g| format!("{tool}(./{g})")))
        .collect()
}

/// `settings` (the text of `.claude/settings.local.json`, empty when there is none) with the
/// deny rules of `old` taken out and those of `new` put in.
pub fn claude_settings(settings: &str, old: &[Pattern], new: &[Pattern]) -> Result<String, String> {
    let mut root: Value = if settings.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(settings).map_err(|e| format!("{CLAUDE} isn't valid JSON: {e}"))?
    };
    let Some(obj) = root.as_object_mut() else {
        return Err(format!("{CLAUDE} isn't a JSON object"));
    };
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(permissions) = permissions.as_object_mut() else {
        return Err(format!("\"permissions\" in {CLAUDE} isn't an object"));
    };
    let deny = permissions
        .entry("deny")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(deny) = deny.as_array_mut() else {
        return Err(format!("\"permissions.deny\" in {CLAUDE} isn't a list"));
    };
    let stale: Vec<String> = old.iter().flat_map(claude_rules).collect();
    deny.retain(|rule| rule.as_str().is_none_or(|r| !stale.iter().any(|s| s == r)));
    for rule in new.iter().flat_map(claude_rules) {
        if !deny.iter().any(|r| r.as_str() == Some(&rule)) {
            deny.push(Value::String(rule));
        }
    }
    serde_json::to_string_pretty(&root)
        .map(|s| s + "\n")
        .map_err(|e| format!("Could not write {CLAUDE}: {e}"))
}

/// `text` with the block between gitcraft's markers (`open … close` lines around them)
/// replaced by `block`, or `block` added at the end; the block taken out when it is empty.
fn replace_block(text: &str, open: &str, close: &str, block: &str) -> String {
    let start_line = format!("{open}{START}{close}");
    let end_line = format!("{open}{END}{close}");
    let mut out: Vec<&str> = Vec::new();
    let mut inside = false;
    let mut placed = false;
    let wrapped = if block.is_empty() {
        String::new()
    } else {
        format!("{start_line}\n{block}{end_line}")
    };
    for line in text.lines() {
        if line.trim() == start_line {
            inside = true;
            continue;
        }
        if inside {
            if line.trim() == end_line {
                inside = false;
                if !placed && !wrapped.is_empty() {
                    out.push(&wrapped);
                }
                placed = true;
            }
            continue;
        }
        out.push(line);
    }
    let mut joined = out.join("\n");
    if !placed && !wrapped.is_empty() {
        if !joined.is_empty() && !joined.ends_with("\n\n") {
            joined.push_str(if joined.ends_with('\n') { "\n" } else { "\n\n" });
        }
        joined.push_str(&wrapped);
    }
    if !joined.is_empty() && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// `.cursorignore` with the fence's patterns in gitcraft's block (same syntax as the fence).
pub fn cursorignore(text: &str, new: &[Pattern]) -> String {
    let block: String = new.iter().map(|p| format!("{}\n", p.text)).collect();
    replace_block(text, "# ", "", &block)
}

/// `AGENTS.md` with a section telling agents (Codex and the others that read it) the paths
/// to stay out of.
pub fn agents_md(text: &str, new: &[Pattern]) -> String {
    let block = if new.is_empty() {
        String::new()
    } else {
        let list: String = new.iter().map(|p| format!("- `{}`\n", p.text)).collect();
        format!(
            "## Paths agents must not touch\n\nDo not read, create, change or delete these \
             paths (patterns as in `.gitignore`), nor run anything that does. Ask instead.\n\n\
             {list}"
        )
    };
    replace_block(text, "<!-- ", " -->", &block)
}

/// Writes the fence into the agents' configurations of the worktree at `root` (on whichever
/// machine it is), `old` being what was written there before. Says which files it changed.
pub fn write_all(
    root: &Path,
    old: &[Pattern],
    new: &[Pattern],
) -> Result<Vec<&'static str>, String> {
    // Never a folder made for a worktree that is gone.
    match crate::remote::try_is_dir(root) {
        Ok(true) => {}
        Ok(false) => return Err("the worktree's folder isn't there".to_owned()),
        Err(e) => return Err(format!("Could not reach the worktree's folder: {e}")),
    }
    let read = |rel: &str| -> Result<String, String> {
        match crate::remote::read_to_string(&crate::remote::join(root, rel)) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(format!("Could not read {rel}: {e}")),
        }
    };
    let mut changed = Vec::new();
    let mut put = |rel: &'static str, before: &str, after: String| -> Result<(), String> {
        // Nothing to say where there was nothing: no file made only to hold an empty list.
        if before == after || (before.is_empty() && new.is_empty()) {
            return Ok(());
        }
        let path = crate::remote::join(root, rel);
        if let Some(parent) = rel
            .rsplit_once('/')
            .map(|(dir, _)| crate::remote::join(root, dir))
        {
            crate::remote::create_dir_all(&parent)
                .map_err(|e| format!("Could not create the folder of {rel}: {e}"))?;
        }
        crate::remote::write(&path, after.as_bytes())
            .map_err(|e| format!("Could not write {rel}: {e}"))?;
        changed.push(rel);
        Ok(())
    };
    let claude = read(CLAUDE)?;
    put(CLAUDE, &claude, claude_settings(&claude, old, new)?)?;
    let cursor = read(CURSOR)?;
    put(CURSOR, &cursor, cursorignore(&cursor, new))?;
    let agents = read(AGENTS)?;
    put(AGENTS, &agents, agents_md(&agents, new))?;
    Ok(changed)
}
