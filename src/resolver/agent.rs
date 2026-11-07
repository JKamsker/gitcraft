//! The agent that resolves: which command line runs it (Claude Code or Codex, headless), what
//! it is told, how it reaches gitcraft (`gitcraft mcp --run <id>`, whose `resolve_*` tools take
//! its reports), and what of its output is shown as it works.

use std::path::Path;

use serde_json::{Value, json};

use super::workspace::Workspace;
use crate::ai::run::CommandLine;
use crate::ai::settings::AiTool;

/// The agents that can resolve in the background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Claude,
    Codex,
}

impl Engine {
    /// The one the AI settings pick; Claude Code unless that is Codex.
    pub fn from_settings(tool: AiTool) -> Self {
        match tool {
            AiTool::Codex => Engine::Codex,
            _ => Engine::Claude,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Engine::Claude => "Claude Code",
            Engine::Codex => "Codex",
        }
    }
}

/// The tools Claude Code may use without asking (nobody is there to ask): reading and editing
/// files in its folder, reading git, and gitcraft's own. Committing, staging, pushing or
/// running anything else is refused.
const CLAUDE_TOOLS: &str = "Read,Edit,MultiEdit,Write,Glob,Grep,LS,TodoWrite,\
Bash(git diff:*),Bash(git log:*),Bash(git show:*),Bash(git status:*),Bash(git blame:*),\
Bash(git ls-files:*),Bash(git merge-base:*),Bash(git rev-parse:*),mcp__gitcraft";

/// How `gitcraft mcp` is started for the agent: `exe`, on `dir`, for the run `run`.
pub fn mcp_args(dir: &Path, run: &str) -> Vec<String> {
    vec![
        "mcp".to_owned(),
        "--repo".to_owned(),
        dir.display().to_string(),
        "--run".to_owned(),
        run.to_owned(),
    ]
}

/// Claude Code's MCP configuration: only gitcraft.
pub fn claude_mcp_config(exe: &Path, dir: &Path, run: &str) -> Value {
    json!({
        "mcpServers": {
            "gitcraft": {
                "type": "stdio",
                "command": exe.display().to_string(),
                "args": mcp_args(dir, run),
            }
        }
    })
}

/// A TOML literal string (`'…'`), for Codex's `-c` settings; none can hold a `'`.
fn toml_literal(text: &str) -> Result<String, String> {
    if text.contains(['\'', '\n', '\r']) {
        return Err(format!(
            "{text} can't be handed to Codex (it has a quote in it)"
        ));
    }
    Ok(format!("'{text}'"))
}

/// The command line that runs `engine` headless. `mcp_file` is where Claude Code's MCP
/// configuration was written; Codex is given it on its command line.
pub fn command(
    engine: Engine,
    exe: &Path,
    dir: &Path,
    run: &str,
    mcp_file: &Path,
) -> Result<CommandLine, String> {
    let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let line = match engine {
        Engine::Claude => {
            let mut w = words(&[
                "claude",
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "acceptEdits",
                "--strict-mcp-config",
                "--mcp-config",
            ]);
            w.push(mcp_file.display().to_string());
            w.push("--allowedTools".to_owned());
            w.push(CLAUDE_TOOLS.to_owned());
            w
        }
        Engine::Codex => {
            let args: Result<Vec<String>, String> =
                mcp_args(dir, run).iter().map(|a| toml_literal(a)).collect();
            let mut w = words(&[
                "codex",
                "exec",
                "--json",
                "--sandbox",
                "workspace-write",
                "--color",
                "never",
            ]);
            w.push("-c".to_owned());
            w.push(format!(
                "mcp_servers.gitcraft.command={}",
                toml_literal(&exe.display().to_string())?
            ));
            w.push("-c".to_owned());
            w.push(format!("mcp_servers.gitcraft.args=[{}]", args?.join(",")));
            w.push("-".to_owned());
            w
        }
    };
    Ok(CommandLine::Words(line))
}

/// What the agent is asked to do.
pub fn prompt(ws: &Workspace) -> String {
    let s = &ws.sides;
    let short = |h: &str| h.get(..10).unwrap_or(h).to_owned();
    let theirs = match (&s.theirs, &s.theirs_name) {
        (Some(h), Some(name)) => format!("{name} ({})", short(h)),
        (Some(h), None) => short(h),
        _ => "the other side".to_owned(),
    };
    let operation = if s.operation.is_empty() {
        "an operation".to_owned()
    } else {
        format!("a {}", s.operation)
    };
    let mut files = String::new();
    for f in &ws.files {
        let n = match f.conflicts {
            0 => "no markers: one side deleted or renamed it".to_owned(),
            1 => "1 conflict".to_owned(),
            n => format!("{n} conflicts"),
        };
        files.push_str(&format!("- {} ({n})\n", f.path));
    }
    format!(
        "You are resolving git conflicts for the user, in the background, in gitcraft (their git \
client). The folder you are in is a private copy of their checkout made for this: {operation} of \
{theirs} into {ours} ({ours_hash}) is in progress here, as it is in theirs. You are already in that folder: run each git command on its own, as it is (`git log -5 MERGE_HEAD`), without `cd`, `&&` or pipes, which aren't allowed here. gitcraft's own tools (log, diff, blame, conflicts) read the repository too.

Conflicted files:
{files}
What to do:
1. First call the gitcraft tool `resolve_plan` with one step per file (id: the path, title: \
what you will do in it, file: the path) and, if it helps, a last step to check the result; give \
`estimate_minutes`, your honest guess of how long all of it takes.
2. For each file: call `resolve_step` with status `running`, read both sides (`git diff`, \
`git log`, `git show` of each side, the code around it), then edit the file so that it keeps \
what each side meant to do, and has no conflict marker (<<<<<<<, =======, >>>>>>>) left. Then \
call `resolve_step` with `done` and a one-line note of how you resolved it, or `failed` with \
why, if you can't resolve it with confidence (leave its markers then).
3. Edit only the conflicted files. Don't stage, commit, continue or abort the {op_word}, and \
don't run anything that changes the repository: gitcraft shows the user your result, and they \
apply it.
4. End by calling `resolve_finish` with `ok` (whether every file is resolved) and a short \
summary for the user: what you decided where the sides disagreed, and anything they should \
check.

Keep notes and the summary short and plain: the user reads them in a small progress panel.",
        ours = s.ours_name,
        ours_hash = short(&s.ours),
        op_word = if s.operation.is_empty() {
            "operation"
        } else {
            &s.operation
        },
    )
}

/// One thing the agent did, as the progress panel lists it.
#[derive(Clone, Debug, PartialEq)]
pub enum Activity {
    /// It used a tool: "Editing src/a.rs".
    Tool(String),
    /// It said something.
    Said(String),
    /// It ended: whether it went well, its last words, and what it cost (Claude Code says).
    Ended {
        ok: bool,
        text: String,
        cost_usd: Option<f64>,
    },
}

/// The most characters of an activity kept.
const MAX_ACTIVITY: usize = 160;

/// `path` relative to `dir` when it is in it, with `/` between its parts.
fn relative(path: &str, dir: &str) -> String {
    let norm = |p: &str| p.replace('\\', "/");
    let (path, dir) = (norm(path), norm(dir));
    let dir = dir.trim_end_matches('/');
    let lower = |s: &str| {
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.to_owned()
        }
    };
    let inside = !dir.is_empty() && lower(&path).starts_with(&format!("{}/", lower(dir)));
    match path.get(dir.len() + 1..) {
        Some(rest) if inside => rest.to_owned(),
        _ => path,
    }
}

/// What using `name` with `input` reads as; `None` for gitcraft's own reports (they show as
/// the steps).
fn tool_use(name: &str, input: &Value, dir: &str) -> Option<String> {
    let field = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    let file = || relative(field("file_path"), dir);
    let text = match name {
        "Read" => format!("Reading {}", file()),
        "Edit" | "MultiEdit" | "Write" => format!("Editing {}", file()),
        "Grep" => format!("Searching for {}", field("pattern")),
        "Glob" | "LS" => format!("Looking for {}", field("pattern")),
        "Bash" => format!("Running {}", field("command")),
        "TodoWrite" => return None,
        n if n.starts_with("mcp__gitcraft__resolve_") => return None,
        n => match n.strip_prefix("mcp__gitcraft__") {
            Some(tool) => format!("Asking gitcraft: {tool}"),
            None => format!("Using {n}"),
        },
    };
    Some(text)
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned()
}

/// What a line of the agent's output (`engine`'s JSON events) tells, for the agent working in
/// `dir`. Several things at once, or nothing worth showing.
pub fn activities(engine: Engine, line: &str, dir: &str) -> Vec<Activity> {
    let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let found = match engine {
        Engine::Claude => claude_event(&event, dir),
        Engine::Codex => codex_event(&event, dir),
    };
    found
        .into_iter()
        .map(|a| match a {
            Activity::Tool(t) => Activity::Tool(super::report::clean(&t, MAX_ACTIVITY)),
            Activity::Said(t) => Activity::Said(super::report::clean(&t, MAX_ACTIVITY)),
            ended => ended,
        })
        .filter(|a| !matches!(a, Activity::Tool(t) | Activity::Said(t) if t.is_empty()))
        .collect()
}

fn claude_event(event: &Value, dir: &str) -> Vec<Activity> {
    match event.get("type").and_then(Value::as_str) {
        Some("assistant") => event
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => Some(Activity::Said(first_line(
                    part.get("text").and_then(Value::as_str).unwrap_or(""),
                ))),
                Some("tool_use") => tool_use(
                    part.get("name").and_then(Value::as_str).unwrap_or(""),
                    part.get("input").unwrap_or(&Value::Null),
                    dir,
                )
                .map(Activity::Tool),
                _ => None,
            })
            .collect(),
        Some("result") => vec![Activity::Ended {
            ok: !event
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && event.get("subtype").and_then(Value::as_str) == Some("success"),
            text: super::report::clean(
                event.get("result").and_then(Value::as_str).unwrap_or(""),
                super::report::MAX_TEXT,
            ),
            cost_usd: event.get("total_cost_usd").and_then(Value::as_f64),
        }],
        _ => Vec::new(),
    }
}

/// A command as the agent wrote it, without the shell Codex wraps it in (`"…\pwsh.exe" -Command
/// '…'`, `bash -lc '…'`).
fn bare_command(command: &str) -> String {
    const SHELLS: [&str; 6] = ["pwsh", "powershell", "bash", "zsh", "sh", "cmd"];
    const FLAGS: [&str; 5] = [" -Command ", " -lc ", " -c ", " /c ", " /C "];
    let Some((program, rest)) = FLAGS.iter().find_map(|f| command.split_once(f)) else {
        return command.to_owned();
    };
    let name = program
        .trim_matches(['"', '\'', ' '])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    if !SHELLS.contains(&name) {
        return command.to_owned();
    }
    let rest = rest.trim();
    let quoted = rest.len() >= 2
        && ((rest.starts_with('\'') && rest.ends_with('\''))
            || (rest.starts_with('"') && rest.ends_with('"')));
    if quoted {
        rest[1..rest.len() - 1].to_owned()
    } else {
        rest.to_owned()
    }
}

fn codex_event(event: &Value, dir: &str) -> Vec<Activity> {
    let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
    let item = event.get("item").unwrap_or(&Value::Null);
    let item_kind = item.get("type").and_then(Value::as_str).unwrap_or("");
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    match (kind, item_kind) {
        ("item.started", "command_execution") => {
            vec![Activity::Tool(format!(
                "Running {}",
                bare_command(&text(item, "command"))
            ))]
        }
        ("item.completed", "file_change") => item
            .get("changes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|c| Activity::Tool(format!("Editing {}", relative(&text(c, "path"), dir))))
            .collect(),
        ("item.started", "mcp_tool_call") => {
            let tool = text(item, "tool");
            if tool.starts_with("resolve_") {
                Vec::new()
            } else {
                vec![Activity::Tool(format!("Asking gitcraft: {tool}"))]
            }
        }
        ("item.completed", "agent_message") => {
            vec![Activity::Said(first_line(&text(item, "text")))]
        }
        ("turn.completed", _) => vec![Activity::Ended {
            ok: true,
            text: String::new(),
            cost_usd: None,
        }],
        ("turn.failed", _) => vec![Activity::Ended {
            ok: false,
            text: event
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Codex stopped")
                .to_owned(),
            cost_usd: None,
        }],
        ("error", _) => vec![Activity::Said(text(event, "message"))],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
