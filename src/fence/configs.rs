//! The fence written into each agent's own configuration in a worktree, so that it is told
//! (and, where the agent can, kept) out of the paths: Claude Code's deny rules in its local
//! settings (always), and, when asked for (they are files a repository usually tracks), a
//! block of Cursor's `.cursorignore` and a section of `AGENTS.md` for Codex and the agents that
//! read it.
//!
//! What gitcraft wrote before is replaced, and nothing else of those files is touched. The
//! rules can leak (a script the agent runs reads the file anyway), so the worktrees are
//! checked as well (see [`super::check`]).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::pattern::Pattern;

/// Where each agent's configuration is, from the worktree's top.
pub const CLAUDE: &str = ".claude/settings.local.json";
pub const CURSOR: &str = ".cursorignore";
pub const AGENTS: &str = "AGENTS.md";

/// Which configurations are written besides Claude Code's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Targets {
    pub cursor: bool,
    pub agents_md: bool,
}

const START: &str = "gitcraft agent fence: start";
const END: &str = "gitcraft agent fence: end";

/// The record of the Claude rules gitcraft wrote, in the worktree's own git folder: what to
/// take out again, whatever the list was when they were written.
const WRITTEN: &str = "gitcraft-fence-rules.json";

/// The Claude Code rules that keep it from reading or changing what `p` fences: the path
/// itself and everything under it. An anchored entry is given from the project's top
/// (`/path`), any other matches at any depth (`**/name`).
pub fn claude_rules(p: &Pattern) -> Vec<String> {
    let body = p.text.trim().trim_start_matches('/').trim_end_matches('/');
    let glob = if p.anchored() {
        format!("/{body}")
    } else {
        format!("**/{body}")
    };
    let globs = [glob.clone(), format!("{glob}/**")];
    ["Read", "Edit"]
        .iter()
        .flat_map(|tool| globs.iter().map(move |g| format!("{tool}({g})")))
        .collect()
}

/// `settings` (the text of `.claude/settings.local.json`, empty when there is none) with the
/// rules in `stale` taken out of its deny list and `rules` put in; `None` when that changes
/// nothing (the file is left as it is, its formatting too).
pub fn claude_settings(
    settings: &str,
    stale: &[String],
    rules: &[String],
) -> Result<Option<String>, String> {
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
    let before = deny.clone();
    deny.retain(|rule| rule.as_str().is_none_or(|r| !stale.iter().any(|s| s == r)));
    for rule in rules {
        if !deny.iter().any(|r| r.as_str() == Some(rule)) {
            deny.push(Value::String(rule.clone()));
        }
    }
    if *deny == before && !settings.trim().is_empty() {
        return Ok(None);
    }
    if settings.trim().is_empty() && rules.is_empty() {
        return Ok(None);
    }
    serde_json::to_string_pretty(&root)
        .map(|s| Some(s + "\n"))
        .map_err(|e| format!("Could not write {CLAUDE}: {e}"))
}

/// `text` with the block between gitcraft's markers (`open … close` lines around them)
/// replaced by `block`, or `block` added at the end; the block taken out when it is empty.
/// A start marker with no end is left as it is, with all that follows. Line ends (`\r\n`)
/// are kept as the file has them.
pub fn replace_block(text: &str, open: &str, close: &str, block: &str) -> String {
    let crlf = text.contains("\r\n");
    let start_line = format!("{open}{START}{close}");
    let end_line = format!("{open}{END}{close}");
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.trim() == start_line);
    let end = start.and_then(|s| {
        lines[s + 1..]
            .iter()
            .position(|l| l.trim() == end_line)
            .map(|e| s + 1 + e)
    });
    let wrapped: Vec<String> = if block.is_empty() {
        Vec::new()
    } else {
        std::iter::once(start_line.clone())
            .chain(block.lines().map(str::to_owned))
            .chain(std::iter::once(end_line.clone()))
            .collect()
    };
    let mut out: Vec<String> = Vec::new();
    match (start, end) {
        (Some(s), Some(e)) => {
            out.extend(lines[..s].iter().map(|l| (*l).to_owned()));
            out.extend(wrapped);
            out.extend(lines[e + 1..].iter().map(|l| (*l).to_owned()));
        }
        _ => {
            out.extend(lines.iter().map(|l| (*l).to_owned()));
            if !wrapped.is_empty() {
                if out.last().is_some_and(|l| !l.trim().is_empty()) {
                    out.push(String::new());
                }
                out.extend(wrapped);
            }
        }
    }
    // A blank line left where the block was taken out from the end.
    while block.is_empty() && out.last().is_some_and(|l| l.trim().is_empty()) && start.is_some() {
        out.pop();
    }
    if out.is_empty() {
        return String::new();
    }
    let nl = if crlf { "\r\n" } else { "\n" };
    out.join(nl) + nl
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

/// A file of the worktree's git data (`git rev-parse --git-path`), on its machine.
fn git_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let out = crate::git::run(Some(root), &["rev-parse", "--git-path", name])
        .map_err(|e| format!("Could not find the worktree's git folder: {e}"))?;
    Ok(crate::remote::resolve(
        root,
        out.trim_end_matches(['\n', '\r']),
    ))
}

fn read_or_empty(path: &Path, what: &str) -> Result<String, String> {
    match crate::remote::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Could not read {what}: {e}")),
    }
}

/// Writes `text` to `rel` of `root`, its folder made first.
fn put(root: &Path, rel: &str, text: &str) -> Result<(), String> {
    if let Some((dir, _)) = rel.rsplit_once('/') {
        crate::remote::create_dir_all(&crate::remote::join(root, dir))
            .map_err(|e| format!("Could not create the folder of {rel}: {e}"))?;
    }
    crate::remote::write(&crate::remote::join(root, rel), text.as_bytes())
        .map_err(|e| format!("Could not write {rel}: {e}"))
}

/// Keeps the local Claude settings out of git (`info/exclude`), as Claude Code does when it
/// makes the file itself: an agent's `git add -A` mustn't commit them.
fn exclude_settings(root: &Path) -> Result<(), String> {
    let exclude = git_path(root, "info/exclude")?;
    let text = read_or_empty(&exclude, "info/exclude")?;
    let line = format!("/{CLAUDE}");
    if text.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    let mut new = text;
    if !new.is_empty() && !new.ends_with('\n') {
        new.push('\n');
    }
    new.push_str(&line);
    new.push('\n');
    if let Some(dir) = exclude.parent() {
        let _ = crate::remote::create_dir_all(dir);
    }
    crate::remote::write(&exclude, new.as_bytes())
        .map_err(|e| format!("Could not write info/exclude: {e}"))
}

/// Writes the fence into the agents' configurations of the worktree at `root` (on whichever
/// machine it is). Says which files it changed.
pub fn write_all(
    root: &Path,
    new: &[Pattern],
    targets: Targets,
) -> Result<Vec<&'static str>, String> {
    // Never a folder made for a worktree that is gone.
    match crate::remote::try_is_dir(root) {
        Ok(true) => {}
        Ok(false) => return Err("the worktree's folder isn't there".to_owned()),
        Err(e) => return Err(format!("Could not reach the worktree's folder: {e}")),
    }
    let mut changed = Vec::new();
    // Claude Code: its local settings, out of git; the rules written recorded beside the
    // worktree's git data.
    let record = git_path(root, WRITTEN)?;
    let stale: Vec<String> =
        serde_json::from_str(&read_or_empty(&record, "the fence's record")?).unwrap_or_default();
    let rules: Vec<String> = new.iter().flat_map(claude_rules).collect();
    let settings = read_or_empty(&crate::remote::join(root, CLAUDE), CLAUDE)?;
    if let Some(text) = claude_settings(&settings, &stale, &rules)? {
        exclude_settings(root)?;
        put(root, CLAUDE, &text)?;
        changed.push(CLAUDE);
    }
    if stale != rules {
        let json = serde_json::to_string(&rules).unwrap_or_else(|_| "[]".to_owned());
        crate::remote::write(&record, json.as_bytes())
            .map_err(|e| format!("Could not record the rules written: {e}"))?;
    }
    // Cursor and AGENTS.md: blocks of their own, taken out again when no longer asked for.
    for (rel, on, make) in [
        (
            CURSOR,
            targets.cursor,
            cursorignore as fn(&str, &[Pattern]) -> String,
        ),
        (AGENTS, targets.agents_md, agents_md),
    ] {
        let path = crate::remote::join(root, rel);
        let before = read_or_empty(&path, rel)?;
        let after = make(&before, if on { new } else { &[] });
        if before != after && !(before.is_empty() && after.trim().is_empty()) {
            put(root, rel, &after)?;
            changed.push(rel);
        }
    }
    Ok(changed)
}
