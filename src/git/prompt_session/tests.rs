use super::*;
#[test]
fn pure_prompt_timestamps_respect_offsets_fractional_seconds_and_calendar_bounds() {
    assert_eq!(parse::stamp("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(
        parse::stamp("2026-11-08T01:30:12.123+01:00"),
        parse::stamp("2026-11-08T00:30:12Z")
    );
    assert_eq!(
        parse::stamp("2024-02-29T23:59:59Z").unwrap() + 1,
        parse::stamp("2024-03-01T00:00:00Z").unwrap()
    );
    for bad in [
        "2025-02-29T00:00:00Z",
        "2024-01-01T25:00:00Z",
        "2024-01-01T00:00:00",
        "2024-01-01T00:00:00+25:00",
        "☃",
    ] {
        assert_eq!(parse::stamp(bad), None, "{bad}");
    }
}
#[test]
fn pure_prompt_claude_allowlist_keeps_prompts_summaries_edits_and_excludes_tool_results_and_reasoning()
 {
    let bytes=br#"{"type":"user","cwd":"/synthetic/root","timestamp":"2026-11-08T00:00:00Z","message":{"role":"user","content":"fix login"}}
{"type":"assistant","cwd":"/synthetic/root","timestamp":"2026-11-08T00:00:01Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"private thought"},{"type":"text","text":"changed login"},{"type":"tool_use","name":"Edit","input":{"file_path":"/synthetic/root/login.rs","old_string":"old","new_string":"new"}}]}}
{"type":"user","cwd":"/synthetic/root","timestamp":"2026-11-08T00:00:02Z","message":{"role":"user","content":[{"type":"tool_result","content":"password=tool-output-never-shown"}]}}
"#;
    let (s, n) = parse::transcript(bytes, Path::new("/synthetic/root"), 0, i64::MAX, true);
    assert!(n.is_empty());
    assert_eq!(s.len(), 3);
    assert_eq!(s[0].kind, "Prompt");
    assert_eq!(s[1].kind, "Agent summary");
    assert_eq!(s[2].path.as_deref(), Some("login.rs"));
    assert_eq!(s[2].before, "old");
    assert_eq!(s[2].after, "new");
    assert!(s[2].exact);
    assert!(!format!("{s:?}").contains("tool-output"));
    assert!(!format!("{s:?}").contains("private thought"));
}
#[test]
fn pure_prompt_codex_patch_steps_never_execute_or_display_shell_arguments() {
    let bytes=br#"{"type":"session_meta","timestamp":"2026-11-08T00:00:00Z","payload":{"cwd":"/synthetic/root"}}
{"type":"response_item","timestamp":"2026-11-08T00:00:01Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix file"}]}}
{"type":"response_item","timestamp":"2026-11-08T00:00:02Z","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: file\n@@\n-old\n+new\n*** End Patch"}}
{"type":"response_item","timestamp":"2026-11-08T00:00:03Z","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"password=do-not-show-or-execute\"}"}}
"#;
    let (s, _) = parse::transcript(bytes, Path::new("/synthetic/root"), 0, i64::MAX, true);
    assert_eq!(s.len(), 3);
    assert_eq!(s[1].path.as_deref(), Some("file"));
    assert_eq!(s[1].after, "new\n");
    assert!(s[2].text.contains("exec_command"));
    assert!(!format!("{s:?}").contains("do-not-show-or-execute"));
}
#[test]
fn pure_prompt_hunk_mapping_refuses_repeated_snippets_wrong_path_and_wrong_side() {
    assert_eq!(
        occurrences("one\nnew\nlast\n", "new\n", false),
        vec![Span {
            start: 2,
            end: 2,
            old: false
        }]
    );
    assert!(occurrences("new\nnew\n", "new\n", false).is_empty());
    let mut e = Evidence {
        commit: "a".repeat(40),
        ..Default::default()
    };
    let mut s = Step::new("Tool", "Edit".into(), None);
    s.path = Some("file".into());
    s.ranges = vec![Span {
        start: 2,
        end: 3,
        old: false,
    }];
    e.sessions.push(Session {
        source: "synthetic".into(),
        relation: "exact recorded content".into(),
        steps: vec![s],
    });
    assert_eq!(hunk_steps(&e, "file", (3, 4), false), vec![(0, 0)]);
    assert!(hunk_steps(&e, "other", (3, 4), false).is_empty());
    assert!(hunk_steps(&e, "file", (3, 4), true).is_empty());
}
#[test]
fn pure_prompt_credential_redaction_drops_multiline_keys_and_token_values_without_losing_line_positions()
 {
    let raw = "safe\napi_key='abcdefgh12345678'\n-----BEGIN PRIVATE KEY-----\nkey body to withhold\n-----END PRIVATE KEY-----\nhttps://user:password@example.com\nsk-proj-abcdefghijklmnopqrstuvwxyz\nsafe end\n";
    let safe = redact(raw);
    assert_eq!(raw.lines().count(), safe.lines().count());
    for key in [
        "abcdefgh12345678",
        "key body",
        "user:password",
        "abcdefghijklmnopqrstuvwxyz",
    ] {
        assert!(!safe.contains(key));
    }
    assert!(safe.contains("safe end"));
    assert!(checkpoint_ref("refs/entire/checkpoints/56/abc123def456"));
    assert!(checkpoint_ref("refs/heads/entire/checkpoints/v1"));
    assert!(!checkpoint_ref("refs/heads/main"));
}

#[test]
fn pure_prompt_local_records_require_both_immutable_worktree_context_and_time_window() {
    let bytes=br#"{"type":"user","cwd":"/synthetic/other","timestamp":"2026-11-08T00:00:00Z","message":{"role":"user","content":"foreign"}}
{"type":"user","cwd":"/synthetic/root","timestamp":"2026-11-07T00:00:00Z","message":{"role":"user","content":"too early"}}
{"type":"user","cwd":"/synthetic/root","timestamp":"2026-11-08T00:00:00Z","message":{"role":"user","content":"matching"}}
{"type":"user","cwd":"/synthetic/root","timestamp":"invalid","message":{"role":"user","content":"unknown time"}}
"#;
    let start = parse::stamp("2026-11-07T23:00:00Z").unwrap();
    let end = parse::stamp("2026-11-08T00:10:00Z").unwrap();
    let (s, _) = parse::records(
        bytes,
        Path::new("/synthetic/root"),
        start,
        end,
        false,
        &|p| Some(PathBuf::from(p)),
    );
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].text, "matching");
}

#[test]
fn pure_prompt_changed_snippet_ranges_exclude_unchanged_patch_context_and_accept_inline_edits() {
    assert_eq!(
        changed_matches(
            "header\ncontext\nnew\ntail\n",
            "context\nnew\ntail\n",
            "context\nold\ntail\n",
            false
        ),
        [Span {
            start: 3,
            end: 3,
            old: false
        }]
    );
    assert!(changed_matches("same\n", "same\n", "same\n", false).is_empty());
    assert_eq!(
        changed_matches("let value = new_value;\n", "new_value", "old_value", false),
        [Span {
            start: 1,
            end: 1,
            old: false
        }]
    );
}

#[test]
fn pure_prompt_checkpoint_offset_keeps_recorded_cwd_but_does_not_attribute_earlier_prompts() {
    let bytes=br#"{"type":"session_meta","timestamp":"2026-11-08T00:00:00Z","payload":{"cwd":"/recorded/agent-worktree"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"earlier prompt"}]}}
{"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: /recorded/agent-worktree/file\n@@\n-old\n+new\n*** End Patch"}}
"#;
    let (s, _) = parse::checkpoint_transcript(bytes, Path::new("/current/other-worktree"), 2);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].path.as_deref(), Some("file"));
    assert!(!format!("{s:?}").contains("earlier prompt"));
    let (s, n) = parse::checkpoint_transcript(bytes, Path::new("/current/other-worktree"), 20);
    assert!(s.is_empty());
    assert!(!n.is_empty());
}

#[test]
fn pure_prompt_checkpoint_push_warning_follows_exact_oid_aliases_and_named_routes() {
    let oid = "a".repeat(40);
    let refs = format!("refs/entire/checkpoints/56/abc123def456\0{oid}\n");
    assert!(checkpoint_publication(
        "refs/heads/copy",
        "refs/heads/export",
        &oid,
        &oid,
        &refs
    ));
    assert!(checkpoint_publication(
        "refs/heads/main",
        "refs/heads/entire/checkpoints/v1",
        "x",
        "x",
        ""
    ));
    assert!(!checkpoint_publication(
        "refs/heads/main",
        "refs/heads/main",
        "b",
        "b",
        &refs
    ));
}
