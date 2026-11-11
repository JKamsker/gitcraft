//! JSONL traversal, context retention, filtering and allowlisted record dispatch.
use super::*;

pub(super) fn records_from(
    bytes: &[u8],
    root: &Path,
    start: i64,
    end: i64,
    embedded: bool,
    canonical: &dyn Fn(&str) -> Option<PathBuf>,
    offset: usize,
) -> (Vec<Step>, Vec<String>) {
    let mut steps = Vec::new();
    let mut notices = Vec::new();
    let mut cwd = String::new();
    let mut at = None;
    for (i, line) in bytes.split(|b| *b == b'\n').enumerate().take(20000) {
        if line.len() > 512 * 1024 {
            notices.push(crate::i18n::message(
                "backend-transcript-line-exceeds-512-kib-omitted-98947e",
                &[("index", (i + 1).to_string().into())],
            ));
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let p = v.get("payload").unwrap_or(&v);
        let next_cwd = string(&v, "cwd");
        let next_cwd = if next_cwd.is_empty() {
            string(p, "cwd")
        } else {
            next_cwd
        };
        if !next_cwd.is_empty() {
            cwd = next_cwd.into();
        }
        let compact =
            v.get("v").and_then(Value::as_u64) == Some(1) && !string(&v, "agent").is_empty();
        let date = string(&v, "timestamp");
        let date = if date.is_empty() {
            let date = string(p, "timestamp");
            if date.is_empty() && compact {
                string(&v, "ts")
            } else {
                date
            }
        } else {
            date
        };
        if !date.is_empty() {
            at = stamp(date);
        }
        if i < offset {
            continue;
        }
        if !embedded
            && (!Path::new(&cwd).is_absolute()
                || (Path::new(&cwd) != root && canonical(&cwd).as_deref() != Some(root))
                || !at.is_some_and(|t| (start..=end).contains(&t)))
        {
            continue;
        }
        let map_root = if embedded && Path::new(&cwd).is_absolute() {
            Path::new(&cwd)
        } else {
            root
        };
        append_record(&mut steps, &v, map_root, &cwd, compact, at, i + 1);
        if steps.len() >= 2000 {
            // One record can contain a summary and many edits or patch files.
            steps.truncate(2000);
            notices.push(
                crate::i18n::text("backend-transcript-stopped-at-2-000-displayed-steps-5ec90c")
                    .into(),
            );
            break;
        }
    }
    if offset > bytes.split(|b| *b == b'\n').count() {
        notices.push(
            crate::i18n::text(
                "backend-checkpoint-offset-exceeds-its-local-transcript-no-earlier-steps-w-2c8c4e",
            )
            .into(),
        );
    }
    if bytes.split(|b| *b == b'\n').count() > 20000 {
        notices.push(
            crate::i18n::text("backend-transcript-stopped-at-20-000-jsonl-lines-4aba31").into(),
        );
    }
    (steps, notices)
}

fn append_record(
    steps: &mut Vec<Step>,
    v: &Value,
    map_root: &Path,
    cwd: &str,
    compact: bool,
    at: Option<i64>,
    line: usize,
) {
    let p = v.get("payload").unwrap_or(v);
    let typ = string(v, "type");
    let payload_type = string(p, "type");
    let message = v.get("message").unwrap_or(p);
    let role = string(message, "role");
    let role = if role.is_empty() && matches!(typ, "user" | "assistant") {
        typ
    } else {
        role
    };
    if matches!(role, "user" | "assistant") {
        let t = text(message.get("content").unwrap_or(&Value::Null), compact);
        if !t.is_empty() {
            steps.push(Step::new(
                if role == "user" {
                    crate::i18n::text("backend-prompt-5c3912")
                } else {
                    crate::i18n::text("backend-agent-summary-8351fe")
                },
                redact(&t),
                at,
            ));
        }
        for block in message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if string(block, "type") == "tool_use" {
                tool(
                    steps,
                    map_root,
                    cwd,
                    string(block, "name"),
                    block.get("input").unwrap_or(&Value::Null),
                    at,
                    line,
                );
            }
        }
    } else if typ == "event_msg" && matches!(payload_type, "user_message" | "agent_message") {
        let t = string(p, "message");
        if !t.is_empty() {
            steps.push(Step::new(
                if payload_type == "user_message" {
                    crate::i18n::text("backend-prompt-5c3912")
                } else {
                    crate::i18n::text("backend-agent-summary-8351fe")
                },
                redact(t),
                at,
            ));
        }
    } else if matches!(payload_type, "function_call" | "custom_tool_call") {
        let name = string(p, "name");
        let raw = string(p, "arguments");
        let raw = if raw.is_empty() {
            string(p, "input")
        } else {
            raw
        };
        if name == "apply_patch" {
            let parsed = serde_json::from_str::<Value>(raw).ok();
            let body = parsed
                .as_ref()
                .map(|p| string(p, "patch"))
                .filter(|s| !s.is_empty())
                .unwrap_or(raw);
            patch(steps, map_root, cwd, body, at, line);
        } else {
            let input = serde_json::from_str::<Value>(raw).unwrap_or(Value::Null);
            tool(steps, map_root, cwd, name, &input, at, line);
        }
    }
}
