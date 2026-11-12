use super::*;
use serde_json::{Value, json};

fn edits(count: usize, summary: bool) -> Value {
    let changes = (0..count)
        .map(|i| json!({"old_string": "before", "new_string": format!("after-{i}")}))
        .collect::<Vec<_>>();
    let mut content = Vec::new();
    if summary {
        content.push(json!({"type": "text", "text": "summary"}));
    }
    content.push(json!({"type": "tool_use", "name": "MultiEdit", "input": {
        "file_path": "/synthetic/root/file", "edits": changes
    }}));
    json!({"type": "assistant", "message": {"role": "assistant", "content": content}})
}

fn patch(count: usize) -> Value {
    let body = (0..count)
        .map(|i| format!("*** Add File: file-{i}\n+after-{i}\n"))
        .collect::<String>();
    json!({"type": "response_item", "payload": {
        "type": "custom_tool_call", "name": "apply_patch", "input": body
    }})
}

#[test]
fn pure_prompt_display_budget_caps_single_edits_batches_summaries_and_patch_files() {
    for (prefix, count) in [
        (1998, 1),
        (1999, 1),
        (2000, 1),
        (1799, 200),
        (1800, 200),
        (1801, 200),
        (1999, 200),
    ] {
        for (record, summary, newline) in [
            (edits(count, false), false, false),
            (edits(count, true), true, false),
            (patch(count), false, true),
        ] {
            let mut lines = vec![json!({"cwd": "/synthetic/root"}).to_string()];
            lines.extend((0..prefix).map(|i| {
                json!({"type": "user", "message": {"content": format!("prompt-{i}")}}).to_string()
            }));
            lines.push(record.to_string());
            let bytes = fixture_records(lines.join("\n").as_bytes());
            let (steps, notices) =
                parse::transcript(&bytes, &fixture_path("/synthetic/root"), 0, i64::MAX, true);
            let total = prefix + count + usize::from(summary);
            assert_eq!(
                steps.len(),
                total.min(2000),
                "{prefix}/{count}/{summary}/{newline}"
            );
            for (i, step) in steps.iter().take(prefix).enumerate() {
                assert_eq!(step.kind, "Prompt");
                assert_eq!(step.text, format!("prompt-{i}"));
            }
            assert_retained_edits(&steps[prefix..], summary, newline);
            let expected = if total >= 2000 {
                vec![crate::i18n::text(
                    "backend-transcript-stopped-at-2-000-displayed-steps-5ec90c",
                )]
            } else {
                vec![]
            };
            assert_eq!(notices, expected);
        }
    }
}

fn assert_retained_edits(steps: &[Step], summary: bool, newline: bool) {
    let mut edits = steps;
    if summary && !steps.is_empty() {
        assert_eq!(steps[0].kind, "Agent summary");
        assert_eq!(steps[0].text, "summary");
        edits = &steps[1..];
    }
    for (i, step) in edits.iter().enumerate() {
        assert_eq!(step.kind, "Tool");
        assert_eq!(
            step.after,
            format!("after-{i}{}", if newline { "\n" } else { "" })
        );
        assert!(step.exact);
    }
}
