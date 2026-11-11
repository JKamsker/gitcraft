//! Allowlisted local transcript fields only. Tool outputs, reasoning, environment and
//! arbitrary tool arguments are not shown. A recorded edit is evidence, never a command.
use super::*;
use serde_json::Value;

pub(super) fn stamp(s: &str) -> Option<i64> {
    if s.len() < 20 || !s.is_ascii() {
        return None;
    }
    if s.bytes()
        .take(19)
        .enumerate()
        .any(|(i, b)| !matches!(i, 4 | 7 | 10 | 13 | 16) && !b.is_ascii_digit())
    {
        return None;
    }
    let n = |a, b| s.get(a..b)?.parse::<i64>().ok();
    if &s[4..5] != "-"
        || &s[7..8] != "-"
        || !matches!(&s[10..11], "T" | " ")
        || &s[13..14] != ":"
        || &s[16..17] != ":"
    {
        return None;
    }
    let (y, m, d, h, min, sec) = (
        n(0, 4)?,
        n(5, 7)?,
        n(8, 10)?,
        n(11, 13)?,
        n(14, 16)?,
        n(17, 19)?,
    );
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let md = match m {
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1970..=9999).contains(&y) || !(1..=md).contains(&d) || h > 23 || min > 59 || sec > 59 {
        return None;
    }
    let year = y - i64::from(m <= 2);
    let era = year.div_euclid(400);
    let yo = year - era * 400;
    let mo = m + if m > 2 { -3 } else { 9 };
    let days = era * 146097 + yo * 365 + yo / 4 - yo / 100 + (153 * mo + 2) / 5 + d - 1 - 719468;
    let zone = &s[19..];
    let zone = if let Some(fraction) = zone.strip_prefix('.') {
        let i = fraction.find(|c: char| !c.is_ascii_digit())?;
        if i == 0 {
            return None;
        }
        &fraction[i..]
    } else {
        zone
    };
    let offset = if zone == "Z" {
        0
    } else if zone.len() == 6 && matches!(&zone[..1], "+" | "-") && &zone[3..4] == ":" {
        let h = zone[1..3].parse::<i64>().ok()?;
        let m = zone[4..].parse::<i64>().ok()?;
        if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
            return None;
        }
        (h * 3600 + m * 60) * if zone.starts_with('-') { -1 } else { 1 }
    } else {
        return None;
    };
    Some(days * 86400 + h * 3600 + min * 60 + sec - offset)
}
fn string<'a>(v: &'a Value, name: &str) -> &'a str {
    v.get(name).and_then(Value::as_str).unwrap_or("")
}
fn text(v: &Value, compact: bool) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|b| {
            matches!(string(b, "type"), "text" | "input_text" | "output_text")
                || compact && string(b, "type").is_empty()
        })
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}
fn relative(root: &Path, cwd: &str, path: &str) -> Option<String> {
    let p = Path::new(path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(cwd).join(p)
    };
    let rel = full.strip_prefix(root).ok()?;
    if rel
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    let s = rel.to_str()?;
    (!s.is_empty() && s.len() < 4096 && !s.chars().any(char::is_control))
        .then(|| s.replace('\\', "/"))
}
fn tool(
    out: &mut Vec<Step>,
    root: &Path,
    cwd: &str,
    name: &str,
    input: &Value,
    at: Option<i64>,
    line: usize,
) {
    if name == "apply_patch" {
        let body = input.as_str().unwrap_or_else(|| {
            let body = string(input, "patch");
            if body.is_empty() {
                string(input, "input")
            } else {
                body
            }
        });
        if !body.is_empty() {
            patch(out, root, cwd, body, at, line);
        } else {
            out.push(Step::new(crate::i18n::text("backend-tool-2e53bd"), crate::i18n::message("backend-apply-patch-transcript-line-line-unmapped-unsupported-recorded-pa-48a294", &[("line", (line).to_string().into())]),at));
        }
        return;
    }
    let path = string(input, "file_path");
    let path = if path.is_empty() {
        string(input, "path")
    } else {
        path
    };
    let edits = if name == "MultiEdit" {
        input
            .get("edits")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        vec![input.clone()]
    };
    for e in edits.into_iter().take(200) {
        let old = string(&e, "old_string");
        let new = if name == "Write" {
            string(&e, "content")
        } else {
            string(&e, "new_string")
        };
        let allowed = matches!(name, "Edit" | "MultiEdit" | "Write");
        let mut step = Step::new(
            crate::i18n::text("backend-tool-2e53bd"),
            crate::i18n::message(
                "backend-name-transcript-line-line-01cffb",
                &[
                    ("name", (name).to_string().into()),
                    ("line", (line).to_string().into()),
                ],
            ),
            at,
        );
        if allowed {
            step.path = relative(root, cwd, path);
            if step.path.is_none() {
                step.text.push_str(if cwd.is_empty() && Path::new(path).is_absolute() { crate::i18n::text("backend-unmapped-recorded-origin-worktree-is-absent-absolute-edit-path-ca-dafb8b") } else { crate::i18n::text("backend-unmapped-recorded-path-is-outside-the-known-worktree-or-unsupport-3d4186") });
            }
            step.before = redact(old);
            step.after = redact(new);
            step.exact = step.before == old && step.after == new;
        }
        out.push(step);
    }
}
fn patch(out: &mut Vec<Step>, root: &Path, cwd: &str, patch: &str, at: Option<i64>, line: usize) {
    let mut path = String::new();
    let mut before = String::new();
    let mut after = String::new();
    let flush = |out: &mut Vec<Step>, path: &str, before: &mut String, after: &mut String| {
        if path.is_empty() || before.is_empty() && after.is_empty() {
            return;
        }
        let mut s = Step::new(
            crate::i18n::text("backend-tool-2e53bd"),
            crate::i18n::message(
                "backend-apply-patch-transcript-line-line-b3793d",
                &[("line", (line).to_string().into())],
            ),
            at,
        );
        s.path = relative(root, cwd, path);
        if s.path.is_none() {
            s.text
                .push_str(if cwd.is_empty() && Path::new(path).is_absolute() {
                    crate::i18n::text("backend-unmapped-recorded-origin-worktree-is-absent-75b8be")
                } else {
                    crate::i18n::text("backend-unmapped-recorded-path-is-outside-the-known-worktree-or-unsupport-3d4186")
                });
        }
        s.before = redact(before);
        s.after = redact(after);
        s.exact = s.before == *before && s.after == *after;
        out.push(s);
        before.clear();
        after.clear();
    };
    for l in patch.lines().take(10000) {
        if let Some(deleted) = l.strip_prefix("*** Delete File: ") {
            flush(out, &path, &mut before, &mut after);
            path.clear();
            let mut step = Step::new(
                crate::i18n::text("backend-tool-2e53bd"),
                crate::i18n::message(
                    "backend-apply-patch-deletion-transcript-line-line-unmapped-removed-bytes-a6920c",
                    &[("line", (line).to_string().into())],
                ),
                at,
            );
            step.path = relative(root, cwd, deleted);
            if step.path.is_none() {
                step.text.push_str(crate::i18n::text(
                    "backend-origin-path-mapping-unavailable-7637d4",
                ));
            }
            out.push(step);
            continue;
        }
        if let Some(p) = l
            .strip_prefix("*** Update File: ")
            .or_else(|| l.strip_prefix("*** Add File: "))
        {
            flush(out, &path, &mut before, &mut after);
            path = p.into();
        } else if l.starts_with("@@") || l == "*** End Patch" {
            flush(out, &path, &mut before, &mut after);
        } else if let Some(l) = l.strip_prefix('+') {
            after.push_str(l);
            after.push('\n');
        } else if let Some(l) = l.strip_prefix('-') {
            before.push_str(l);
            before.push('\n');
        } else if let Some(l) = l.strip_prefix(' ') {
            before.push_str(l);
            before.push('\n');
            after.push_str(l);
            after.push('\n');
        }
    }
    flush(out, &path, &mut before, &mut after);
}
/// `embedded` is true only for an immutable checkpoint linked by an exact commit trailer.
/// Local records instead require both a matching canonical worktree and a bounded time.
pub(super) fn transcript(
    bytes: &[u8],
    root: &Path,
    start: i64,
    end: i64,
    embedded: bool,
) -> (Vec<Step>, Vec<String>) {
    records(bytes, root, start, end, embedded, &|p| {
        std::fs::canonicalize(p).ok()
    })
}
pub(super) fn records(
    bytes: &[u8],
    root: &Path,
    start: i64,
    end: i64,
    embedded: bool,
    canonical: &dyn Fn(&str) -> Option<PathBuf>,
) -> (Vec<Step>, Vec<String>) {
    records_from(bytes, root, start, end, embedded, canonical, 0)
}
pub(super) fn checkpoint_transcript(
    bytes: &[u8],
    root: &Path,
    offset: usize,
) -> (Vec<Step>, Vec<String>) {
    records_from(
        bytes,
        root,
        i64::MIN,
        i64::MAX,
        true,
        &|p| Some(PathBuf::from(p)),
        offset,
    )
}
fn records_from(
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
        let typ = string(&v, "type");
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
                        &mut steps,
                        map_root,
                        &cwd,
                        string(block, "name"),
                        block.get("input").unwrap_or(&Value::Null),
                        at,
                        i + 1,
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
                patch(&mut steps, map_root, &cwd, body, at, i + 1);
            } else {
                let input = serde_json::from_str::<Value>(raw).unwrap_or(Value::Null);
                tool(&mut steps, map_root, &cwd, name, &input, at, i + 1);
            }
        }
        if steps.len() >= 2000 {
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
