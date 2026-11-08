//! Entire public tree format: refs backend first, legacy aggregate branch second.
//! Reference: entireio/cli api/checkpoint/metadata.go and docs/architecture/{sessions-
//! and-checkpoints,ref-checkpoint-backend}.md. No CLI or repository hook is executed.
use super::*;
use serde_json::Value;
fn id(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| b.is_ascii_hexdigit())
        || s.len() == 26
            && s.bytes()
                .all(|b| b.is_ascii_digit() || b"ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
}
pub(super) fn trailers(message: &str) -> Vec<String> {
    // The subject is never a checkpoint link. Only the final trailer paragraph counts.
    message
        .trim_end()
        .rsplit_once("\n\n")
        .map(|(_, block)| {
            block
                .lines()
                .filter_map(|line| {
                    let value = line.strip_prefix("Entire-Checkpoint:")?.trim();
                    id(value).then(|| value.to_owned())
                })
                .take(16)
                .collect()
        })
        .unwrap_or_default()
}
fn path(v: &Value, key: &str, prefix: &str) -> Option<String> {
    let p = v.get(key)?.as_str()?.trim_start_matches('/');
    if p.is_empty()
        || !p.starts_with(prefix)
        || p.split('/')
            .any(|n| n.is_empty() || matches!(n, "." | ".."))
        || p.chars().any(char::is_control)
    {
        return None;
    }
    Some(p.into())
}
fn json(
    repo: &Path,
    rev: &str,
    path: &str,
    active: &dyn Fn() -> bool,
    budget: &mut usize,
) -> Result<Value, String> {
    serde_json::from_slice(&object(repo, rev, path, active, budget)?)
        .map_err(|_| "Checkpoint metadata JSON is invalid".into())
}
pub(super) fn load(
    repo: &Path,
    message: &str,
    root: &Path,
    active: &dyn Fn() -> bool,
    budget: &mut usize,
    out: &mut Evidence,
) {
    for checkpoint in trailers(message) {
        if !active() {
            return;
        }
        let reference = format!(
            "refs/entire/checkpoints/{}/{}",
            &checkpoint[checkpoint.len() - 2..],
            checkpoint
        );
        let pinned = read(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ],
            active,
        )
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_owned());
        let (rev, prefix) = if let Some(oid) = pinned {
            (oid, String::new())
        } else {
            let legacy = read(
                repo,
                &[
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    "refs/heads/entire/checkpoints/v1^{commit}",
                ],
                active,
            )
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim().to_owned());
            let Some(oid) = legacy else {
                out.notices.push(format!("Entire checkpoint {checkpoint} is linked but unavailable locally; fetch it explicitly. No transcript was downloaded"));
                continue;
            };
            (oid, format!("{}/{}/", &checkpoint[..2], &checkpoint[2..]))
        };
        let summary = match json(
            repo,
            &rev,
            &format!("{prefix}metadata.json"),
            active,
            budget,
        ) {
            Ok(s) => s,
            Err(e) => {
                out.notices.push(e);
                continue;
            }
        };
        if summary.get("checkpoint_id").and_then(Value::as_str) != Some(&checkpoint) {
            out.notices
                .push("Checkpoint root identity does not match its exact commit trailer".into());
            continue;
        }
        let Some(sessions) = summary.get("sessions").and_then(Value::as_array) else {
            out.notices
                .push("Checkpoint has no supported session-path metadata".into());
            continue;
        };
        if sessions.len() > 32 {
            out.notices
                .push("Checkpoint read limited to 32 sessions".into());
        }
        for entry in sessions.iter().take(32) {
            if !active() {
                return;
            }
            let Some(metadata) = path(entry, "metadata", &prefix) else {
                out.notices
                    .push("Invalid checkpoint session metadata path".into());
                continue;
            };
            // Both paths must remain in this numbered session's own directory.
            let folder = metadata.rsplit_once('/').map_or("", |p| p.0);
            if folder.is_empty() {
                out.notices
                    .push("Checkpoint session directory is unavailable".into());
                continue;
            }
            let own = format!("{folder}/");
            let session_meta = match json(repo, &rev, &metadata, active, budget) {
                Ok(m) => m,
                Err(e) => {
                    out.notices.push(e);
                    continue;
                }
            };
            if session_meta.get("checkpoint_id").and_then(Value::as_str) != Some(&checkpoint) {
                out.notices
                    .push("Session checkpoint identity mismatch; omitted".into());
                continue;
            }
            let mut steps = Vec::new();
            if let Some(prompt) = path(entry, "prompt", &own)
                .and_then(|p| object(repo, &rev, &p, active, budget).ok())
                .and_then(|b| String::from_utf8(b).ok())
            {
                steps.push(Step::new("Checkpoint prompts", prompt, None));
            }
            if let Some(summary) = session_meta.get("summary") {
                let fields = ["intent", "outcome", "learnings", "friction", "open_items"]
                    .into_iter()
                    .filter_map(|key| {
                        summary.get(key).map(|v| {
                            format!(
                                "{key}: {}",
                                v.as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| v.to_string())
                            )
                        })
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !fields.is_empty() {
                    steps.push(Step::new("Checkpoint summary", fields, None));
                }
            }
            let raw = path(entry, "transcript", &own)
                .and_then(|p| object(repo, &rev, &p, active, budget).ok());
            let (transcript, offset) = if let Some(raw) = raw {
                (
                    Some(raw),
                    session_meta
                        .get("checkpoint_transcript_start")
                        .or_else(|| session_meta.get("transcript_lines_at_start"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                )
            } else {
                out.notices.push("Raw Entire transcript unavailable or over bounds; trying its recorded compact transcript. Chunked raw transcripts over 4 MiB are not loaded".into());
                (
                    path(entry, "compact_transcript", &own)
                        .and_then(|p| object(repo, &rev, &p, active, budget).ok()),
                    session_meta
                        .get("compact_transcript_start")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                )
            };
            if let Some(bytes) = transcript {
                let (parsed, notices) = parse::checkpoint_transcript(&bytes, root, offset);
                steps.extend(parsed);
                out.notices.extend(notices);
            } else {
                out.notices.push(
                    "No bounded supported transcript available for this checkpoint session".into(),
                );
            }
            let id = session_meta
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown session");
            out.sessions.push(Session{source:format!("Entire {checkpoint} / {}",redact(id)),relation:format!("Exact Entire-Checkpoint trailer; immutable checkpoint tree {rev}. Imported commit anchors without a trailer are not treated as authorship"),steps});
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_prompt_checkpoint_trailers_and_offsets_exclude_subject_body_and_previous_turns() {
        assert!(trailers("Entire-Checkpoint: abc123def456\n").is_empty());
        assert!(trailers("subject\n\nEntire-Checkpoint: abc123def456\n\nordinary body").is_empty());
        assert_eq!(
            trailers("subject\n\nEntire-Checkpoint: abc123def456\nSigned-off-by: Human"),
            ["abc123def456"]
        );
        assert!(trailers("subject\n\nEntire-Checkpoint: ../private").is_empty());
        assert!(
            path(
                &serde_json::json!({"metadata":"/ab/c/0/../other"}),
                "metadata",
                "ab/c/"
            )
            .is_none()
        );
        assert!(
            path(
                &serde_json::json!({"transcript":"/different/0/full.jsonl"}),
                "transcript",
                "ab/c/0/"
            )
            .is_none()
        );
    }
}
