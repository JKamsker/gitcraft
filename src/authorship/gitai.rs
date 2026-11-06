//! git-ai's authorship log (`refs/notes/ai`, standard 3.0.0): per file, which lines an AI
//! session (`s_…::t_…`, or a legacy 16-hex prompt) or a known human (`h_…`) wrote. Lines no key
//! covers have no known author.

use super::{FileLines, Who};

/// What a note says, by file: ranges of the commit's lines (new side, from 1) and who wrote
/// them; and the tools of its sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub files: FileLines,
    pub tools: Vec<String>,
}

/// Reads a note; `None` when it isn't one of git-ai's.
pub fn parse(text: &str) -> Option<Note> {
    let (attestation, meta) = match text.split_once("\n---\n") {
        Some((a, m)) => (a, m),
        None => text.strip_suffix("\n---").map(|a| (a, ""))?,
    };
    let meta: serde_json::Value = serde_json::from_str(meta.trim()).unwrap_or_default();
    if meta
        .get("schema_version")
        .and_then(|v| v.as_str())
        .is_some_and(|v| !v.starts_with("authorship/"))
    {
        return None;
    }
    let mut note = Note::default();
    let mut file: Option<String> = None;
    for line in attestation.lines() {
        if let Some(entry) = line.strip_prefix("  ") {
            let (Some(path), Some((key, ranges))) = (&file, entry.trim().split_once(' ')) else {
                continue;
            };
            let who = who_of(key, &meta);
            let list = note.files.entry(path.clone()).or_default();
            list.extend(parse_ranges(ranges).into_iter().map(|(a, b)| (a, b, who)));
        } else if !line.trim().is_empty() {
            file = Some(unquote(line.trim_end()));
        }
    }
    for tools in [&meta["sessions"], &meta["prompts"]] {
        if let Some(map) = tools.as_object() {
            for v in map.values() {
                if let Some(tool) = v.pointer("/agent_id/tool").and_then(|t| t.as_str())
                    && !note.tools.iter().any(|t| t == tool)
                {
                    note.tools.push(tool.to_owned());
                }
            }
        }
    }
    Some(note)
}

/// Who an attestation key stands for.
fn who_of(key: &str, meta: &serde_json::Value) -> Who {
    if key.starts_with("h_") {
        return Who::Human;
    }
    // `s_…::t_…` resolves through `sessions`, a legacy key through `prompts`; both are AI.
    let session = key.split("::").next().unwrap_or(key);
    let known = meta["sessions"].get(session).is_some() || meta["prompts"].get(key).is_some();
    if known || key.starts_with("s_") || key.len() == 16 {
        Who::Agent
    } else {
        Who::Unknown
    }
}

/// `1-10,15,20-22` as inclusive ranges.
fn parse_ranges(text: &str) -> Vec<(u32, u32)> {
    text.split(',')
        .filter_map(|part| {
            let part = part.trim();
            match part.split_once('-') {
                Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
                None => part.parse().ok().map(|n| (n, n)),
            }
        })
        .filter(|(a, b)| a <= b && *a > 0)
        .collect()
}

/// A path written in double quotes (it has spaces, tabs or a line break).
fn unquote(path: &str) -> String {
    match path.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
        Some(inner) => inner
            .replace("\\\"", "\"")
            .replace("\\t", "\t")
            .replace("\\n", "\n")
            .replace("\\\\", "\\"),
        None => path.to_owned(),
    }
}
