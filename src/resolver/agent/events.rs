use super::*;

pub(super) fn relative(path: &str, dir: &str) -> String {
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
pub(super) fn tool_use(name: &str, input: &Value, dir: &str) -> Option<String> {
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

pub(super) fn first_line(text: &str) -> String {
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
            Activity::Tool(t) => Activity::Tool(super::super::report::clean(&t, MAX_ACTIVITY)),
            Activity::Said(t) => Activity::Said(super::super::report::clean(&t, MAX_ACTIVITY)),
            Activity::Ended { ok, text, cost_usd } => Activity::Ended {
                ok,
                text: super::super::report::clean(&text, super::super::report::MAX_TEXT),
                cost_usd,
            },
        })
        .filter(|a| !matches!(a, Activity::Tool(t) | Activity::Said(t) if t.is_empty()))
        .collect()
}

pub(super) fn claude_event(event: &Value, dir: &str) -> Vec<Activity> {
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
            text: super::super::report::clean(
                event.get("result").and_then(Value::as_str).unwrap_or(""),
                super::super::report::MAX_TEXT,
            ),
            cost_usd: event.get("total_cost_usd").and_then(Value::as_f64),
        }],
        _ => Vec::new(),
    }
}

/// A command as the agent wrote it, without the shell Codex wraps it in (`"…\pwsh.exe" -Command
/// '…'`, `bash -lc '…'`).
pub(super) fn bare_command(command: &str) -> String {
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

pub(super) fn codex_event(event: &Value, dir: &str) -> Vec<Activity> {
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
