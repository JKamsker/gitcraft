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
        .map_err(|_| crate::i18n::text("backend-checkpoint-metadata-json-is-invalid-3b8923").into())
}
fn recorded_link(summary: &Value, commit: &str) -> bool {
    summary
        .get("linked_commits")
        .and_then(Value::as_array)
        .is_some_and(|links| {
            links
                .iter()
                .take(4096)
                .any(|link| link.get("sha").and_then(Value::as_str) == Some(commit))
        })
}
// Local object metadata only. Scan at most 256 checkpoint roots and charge every
// metadata object to the same evidence budget. Import commit_sha is never a link.
fn discover(
    repo: &Path,
    commit: &str,
    active: &dyn Fn() -> bool,
    budget: &mut usize,
    out: &mut Evidence,
) -> Vec<(String, String, String, Value)> {
    let rows = match read(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)",
            "refs/entire/checkpoints/",
            "refs/heads/entire/checkpoints/v1",
        ],
        active,
    ) {
        Ok(rows) if rows.len() <= 1024 * 1024 => rows,
        _ => {
            out.notices.push(crate::i18n::text("backend-exact-sha-entire-attachment-lookup-unavailable-or-over-its-1-mib-5e76c8").into());
            return vec![];
        }
    };
    let mut candidates = Vec::new();
    let mut omitted = false;
    for row in String::from_utf8_lossy(&rows).lines() {
        let Some((name, rev)) = row.split_once('\0') else {
            continue;
        };
        if !git::investigations::oid(rev) {
            continue;
        }
        if name == "refs/heads/entire/checkpoints/v1" {
            let paths = match read(repo, &["ls-tree", "-r", "--name-only", "-z", rev], active) {
                Ok(paths) if paths.len() <= 1024 * 1024 => paths,
                _ => {
                    omitted = true;
                    continue;
                }
            };
            for path in paths
                .split(|b| *b == 0)
                .filter_map(|p| std::str::from_utf8(p).ok())
            {
                let fields = path.split('/').collect::<Vec<_>>();
                if fields.len() != 3 || fields[0].len() != 2 || fields[2] != "metadata.json" {
                    continue;
                }
                let checkpoint = format!("{}{}", fields[0], fields[1]);
                if !id(&checkpoint) {
                    continue;
                }
                if candidates.len() >= 256 {
                    omitted = true;
                    break;
                }
                candidates.push((
                    checkpoint,
                    rev.to_owned(),
                    format!("{}/{}/", fields[0], fields[1]),
                ));
            }
        } else if let Some(checkpoint) = name.rsplit('/').next().filter(|id_| id(id_)) {
            if name
                != format!(
                    "refs/entire/checkpoints/{}/{}",
                    &checkpoint[checkpoint.len() - 2..],
                    checkpoint
                )
            {
                continue;
            }
            if candidates.len() >= 256 {
                omitted = true;
                break;
            }
            candidates.push((checkpoint.to_owned(), rev.to_owned(), String::new()));
        }
    }
    let mut matches = Vec::new();
    for (checkpoint, rev, prefix) in candidates {
        if !active() || *budget == 0 {
            omitted = true;
            break;
        }
        match json(
            repo,
            &rev,
            &format!("{prefix}metadata.json"),
            active,
            budget,
        ) {
            Ok(summary)
                if summary.get("checkpoint_id").and_then(Value::as_str) == Some(&checkpoint)
                    && recorded_link(&summary, commit) =>
            {
                matches.push((checkpoint, rev, prefix, summary))
            }
            Ok(summary) => {
                if summary
                    .get("linked_commits")
                    .and_then(Value::as_array)
                    .is_some_and(|links| links.len() > 4096)
                {
                    omitted = true;
                }
            }
            Err(_) => omitted = true,
        }
    }
    if omitted {
        out.notices.push(
            crate::i18n::text(
                "backend-exact-sha-entire-attachment-lookup-was-partial-maximum-256-checkp-c9cdc0",
            )
            .into(),
        );
    }
    matches
}
pub(super) fn load(
    repo: &Path,
    message: &str,
    commit: &str,
    root: &Path,
    active: &dyn Fn() -> bool,
    budget: &mut usize,
    out: &mut Evidence,
) {
    let trailers = trailers(message);
    let mut queue = trailers
        .iter()
        .cloned()
        .map(|id| (id, None))
        .collect::<std::collections::VecDeque<_>>();
    let mut discovered = false;
    loop {
        if queue.is_empty() {
            if discovered {
                break;
            }
            discovered = true;
            queue.extend(
                discover(repo, commit, active, budget, out)
                    .into_iter()
                    .filter(|item| !trailers.contains(&item.0))
                    .map(|(id, rev, prefix, summary)| (id, Some((rev, prefix, summary)))),
            );
        }
        let Some((checkpoint, attachment)) = queue.pop_front() else {
            break;
        };
        if !active() {
            return;
        }
        let is_attachment = attachment.is_some();
        let (rev, prefix, attached_summary) = if let Some((rev, prefix, summary)) = attachment {
            (rev, prefix, Some(summary))
        } else {
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
                    out.notices.push(crate::i18n::message("backend-entire-checkpoint-checkpoint-is-linked-but-unavailable-locally-fe-69bccd", &[("checkpoint", (checkpoint).to_string().into())]));
                    continue;
                };
                (oid, format!("{}/{}/", &checkpoint[..2], &checkpoint[2..]))
            };
            (rev, prefix, None)
        };
        let summary = match attached_summary.map(Ok).unwrap_or_else(|| {
            json(
                repo,
                &rev,
                &format!("{prefix}metadata.json"),
                active,
                budget,
            )
        }) {
            Ok(s) => s,
            Err(e) => {
                out.notices.push(e);
                continue;
            }
        };
        if summary.get("checkpoint_id").and_then(Value::as_str) != Some(&checkpoint) {
            out.notices.push(
                crate::i18n::text(
                    "backend-checkpoint-root-identity-does-not-match-its-recorded-link-028c11",
                )
                .into(),
            );
            continue;
        }
        let Some(sessions) = summary.get("sessions").and_then(Value::as_array) else {
            out.notices.push(
                crate::i18n::text(
                    "backend-checkpoint-has-no-supported-session-path-metadata-f08b18",
                )
                .into(),
            );
            continue;
        };
        if sessions.len() > 32 {
            out.notices.push(
                crate::i18n::text("backend-checkpoint-read-limited-to-32-sessions-27927c").into(),
            );
        }
        for entry in sessions.iter().take(32) {
            if !active() {
                return;
            }
            let Some(metadata) = path(entry, "metadata", &prefix) else {
                out.notices.push(
                    crate::i18n::text("backend-invalid-checkpoint-session-metadata-path-e0c0d0")
                        .into(),
                );
                continue;
            };
            // Both paths must remain in this numbered session's own directory.
            let folder = metadata.rsplit_once('/').map_or("", |p| p.0);
            if folder.is_empty() {
                out.notices.push(
                    crate::i18n::text("backend-checkpoint-session-directory-is-unavailable-bc96ec")
                        .into(),
                );
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
                out.notices.push(
                    crate::i18n::text(
                        "backend-session-checkpoint-identity-mismatch-omitted-12f86c",
                    )
                    .into(),
                );
                continue;
            }
            let mut steps = Vec::new();
            if let Some(prompt) = path(entry, "prompt", &own)
                .and_then(|p| object(repo, &rev, &p, active, budget).ok())
                .and_then(|b| String::from_utf8(b).ok())
            {
                steps.push(Step::new(
                    crate::i18n::text("backend-checkpoint-prompts-556698"),
                    prompt,
                    None,
                ));
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
                    steps.push(Step::new(
                        crate::i18n::text("backend-checkpoint-summary-e42ba2"),
                        fields,
                        None,
                    ));
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
                out.notices.push(crate::i18n::text("backend-raw-entire-transcript-unavailable-or-over-bounds-trying-its-recor-98c4c6").into());
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
                    crate::i18n::text("backend-no-bounded-supported-transcript-available-for-this-checkpoint-ses-a4c187").into(),
                );
            }
            let id = session_meta
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or(crate::i18n::text("backend-unknown-session-52813e"));
            out.sessions.push(Session{source:format!("Entire {checkpoint} / {}",redact(id)),relation:crate::i18n::message("backend-immutable-checkpoint-tree-rev-recorded-evidence-is-unverified-imp-2b2b29", &[("if", (if is_attachment {"Unverified recorded exact-SHA linked_commits attachment"} else {"Recorded Entire-Checkpoint trailer"}).to_string().into()), ("rev", (rev).to_string().into())]),steps});
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_prompt_r1_exact_sha_recorded_attachment_excludes_import_anchors_and_prefixes() {
        let sha = "a".repeat(40);
        assert!(recorded_link(
            &serde_json::json!({"linked_commits":[{"sha":sha,"repo":"gh/example/repository"}]}),
            &sha
        ));
        for summary in [
            serde_json::json!({"commit_sha":sha}),
            serde_json::json!({"linked_commits":[{"sha":"aaaa"}]}),
            serde_json::json!({"linked_commits":[{"sha":"b".repeat(40)}]}),
        ] {
            assert!(!recorded_link(&summary, &sha));
        }
    }
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
