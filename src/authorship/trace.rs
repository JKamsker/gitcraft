//! Agent Trace records (agent-trace.dev, 0.1): per file, conversations whose contributor (`ai`,
//! `human`, `mixed`, `unknown`) wrote some ranges of lines, at a revision. Read from the notes
//! ref `refs/notes/agent-trace` (one or more records per commit, as JSON or JSON lines).

use super::{FileLines, Who};

fn who_of(kind: Option<&str>) -> Who {
    match kind {
        Some("ai") => Who::Agent,
        Some("human") => Who::Human,
        Some("mixed") => Who::Mixed,
        _ => Who::Unknown,
    }
}

/// What the records in `text` say by file, and the tools they name.
pub fn parse(text: &str) -> Option<(FileLines, Vec<String>)> {
    let records: Vec<serde_json::Value> = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Array(list)) => list,
        Ok(one @ serde_json::Value::Object(_)) => vec![one],
        _ => text
            .lines()
            .filter_map(|l| serde_json::from_str(l.trim()).ok())
            .collect(),
    };
    let mut files = FileLines::default();
    let mut tools = Vec::new();
    for record in &records {
        if let Some(tool) = record.pointer("/tool/name").and_then(|t| t.as_str())
            && !tools.iter().any(|t: &String| t == tool)
        {
            tools.push(tool.to_owned());
        }
        for file in record["files"].as_array().into_iter().flatten() {
            let Some(path) = file["path"].as_str() else {
                continue;
            };
            for conv in file["conversations"].as_array().into_iter().flatten() {
                let default = who_of(conv.pointer("/contributor/type").and_then(|t| t.as_str()));
                for range in conv["ranges"].as_array().into_iter().flatten() {
                    let (Some(a), Some(b)) =
                        (range["start_line"].as_u64(), range["end_line"].as_u64())
                    else {
                        continue;
                    };
                    // A range may say who wrote it itself.
                    let who = range
                        .pointer("/contributor/type")
                        .and_then(|t| t.as_str())
                        .map_or(default, |t| who_of(Some(t)));
                    let (Ok(a), Ok(b)) = (u32::try_from(a), u32::try_from(b)) else {
                        continue;
                    };
                    if a >= 1 && a <= b {
                        files.entry(path.to_owned()).or_default().push((a, b, who));
                    }
                }
            }
        }
    }
    (!records.is_empty()).then_some((files, tools))
}
