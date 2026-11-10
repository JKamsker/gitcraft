use super::*;
fn recorded_scan(
    config: &Config,
    blob: &[u8],
    size_override: Option<usize>,
    truncate: bool,
) -> Vec<Check> {
    let id = "a".repeat(40);
    let size = size_override.unwrap_or(blob.len());
    let mut stage = 0;
    let result = residual_checks(config, |args, input, limit| {
        // Both fields are synthetic canaries. Neither may occur in any argv.
        for rule in &config.replacements {
            assert!(!args.join(" ").contains(&rule.find));
            if !rule.replace.is_empty() {
                assert!(!args.join(" ").contains(&rule.replace));
            }
        }
        stage += 1;
        let output = match stage {
            1 => {
                assert_eq!(
                    args,
                    ["rev-list", "--objects", "--all", "--no-object-names"]
                );
                assert!(input.is_empty());
                format!("{id}\n").into_bytes()
            }
            2 => {
                assert_eq!(args, ["cat-file", "--batch-check"]);
                assert_eq!(input, format!("{id}\n").as_bytes());
                format!("{id} blob {size}\n").into_bytes()
            }
            3 => {
                assert_eq!(args, ["cat-file", "--batch"]);
                assert_eq!(input, format!("{id}\n").as_bytes());
                let mut out = format!("{id} blob {size}\n").into_bytes();
                out.extend_from_slice(blob);
                if !truncate {
                    out.push(b'\n');
                }
                out
            }
            _ => return Err("Unexpected command".into()),
        };
        assert!(output.len() <= limit);
        Ok(output)
    });
    assert_eq!(stage, if size > SCAN_BLOB { 2 } else { 3 });
    result
}
#[test]
fn residual_recording_keeps_both_sensitive_fields_out_of_argv() {
    let config = Config {
        paths: Vec::new(),
        replacements: vec![Replacement {
            find: "GC_REVIEW_CANARY_DO_NOT_USE".into(),
            replace: "GC_REPLACEMENT_CANARY".into(),
        }],
    };
    let hit = recorded_scan(
        &config,
        b"prefix GC_REVIEW_CANARY_DO_NOT_USE suffix",
        None,
        false,
    );
    assert!(!hit[0].passed);
    assert!(hit[0].detail.contains("remains"));
    let clean = recorded_scan(&config, b"prefix GC_REPLACEMENT_CANARY suffix", None, false);
    assert!(clean[0].passed);
    let binary = recorded_scan(&config, b"\0binary", None, false);
    assert!(!binary[0].passed);
    assert!(binary[0].detail.contains("Unverified"));
    let big = recorded_scan(&config, b"", Some(SCAN_BLOB + 1), false);
    assert!(!big[0].passed);
    assert!(big[0].detail.contains("4 MiB"));
    let truncated = recorded_scan(&config, b"text", None, true);
    assert!(!truncated[0].passed);
    assert!(truncated[0].detail.contains("Unverified"));
}
#[test]
fn residual_budgets_refuse_before_blob_download_and_errors_are_redacted() {
    let config = Config {
        paths: Vec::new(),
        replacements: vec![Replacement {
            find: "SENSITIVE_FIND_CANARY".into(),
            replace: "SENSITIVE_REPLACE_CANARY".into(),
        }],
    };
    for count in [5, SCAN_OBJECTS + 1] {
        let ids: Vec<_> = (0..count).map(|i| format!("{i:040x}")).collect();
        let mut calls = 0;
        let result = residual_checks(&config, |args, _, limit| {
            calls += 1;
            let data = match args {
                ["rev-list", ..] => format!("{}\n", ids.join("\n")),
                ["cat-file", "--batch-check"] => ids
                    .iter()
                    .map(|id| format!("{id} blob {SCAN_BLOB}\n"))
                    .collect(),
                _ => {
                    return Err("Unexpected blob download after budget refusal".into());
                }
            };
            assert!(data.len() <= limit);
            Ok(data.into_bytes())
        });
        assert!(!result[0].passed);
        assert!(
            result[0]
                .detail
                .contains(if count == 5 { "16 MiB" } else { "20000" })
        );
        assert_eq!(calls, if count == 5 { 2 } else { 1 });
    }
    let checks = residual_checks(&config, |_, _, _| {
        Err("SENSITIVE_FIND_CANARY SENSITIVE_REPLACE_CANARY".into())
    });
    assert!(!checks[0].passed);
    assert!(!checks[0].detail.contains("SENSITIVE_FIND_CANARY"));
    assert!(!checks[0].detail.contains("SENSITIVE_REPLACE_CANARY"));
}
#[test]
fn literal_scan_handles_overlaps_and_non_utf8_without_regex() {
    for (pattern, text, found) in [
        (&b"ababac"[..], &b"ababababac"[..], true),
        (b"a.*", b"abc", false),
        (b"token", b"\xfftoken\xfe", true),
        (b"token", b"toke", false),
    ] {
        assert_eq!(Literal::new(pattern).contains(text), found);
    }
}

#[test]
fn config_is_bounded_and_literal_without_force_or_publication() -> Result<(), String> {
    let config = Config {
        paths: vec!["-old/secret".into(), "legacy/".into()],
        replacements: vec![Replacement {
            find: "regex:token.*".into(),
            replace: "gone$1".into(),
        }],
    };
    let args = config.args(Some(Path::new("/recorded/rules")))?;
    assert_eq!(
        args,
        [
            "filter-repo",
            "--invert-paths",
            "--path=-old/secret",
            "--path=legacy/",
            "--replace-text",
            "/recorded/rules"
        ]
    );
    assert_eq!(
        config.replacement_text(),
        "literal:regex:token.*==>gone$1\n"
    );
    for path in ["../secret", "/secret", ".git/config"] {
        assert!(
            Config {
                paths: vec![path.into()],
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(Config::default().validate().is_err());
    assert!(
        Config {
            paths: vec!["a".into(); 33],
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    Ok(())
}
#[test]
fn ref_report_includes_deleted_added_and_rewritten_refs() -> Result<(), String> {
    let a = "a".repeat(40);
    let b = "b".repeat(64);
    let before = parse_refs(&format!("refs/heads/main\0{a}\nrefs/tags/old\0{a}\n"))?;
    let after = parse_refs(&format!("refs/heads/main\0{b}\nrefs/tags/new\0{b}\n"))?;
    let changed = changes(&before, &after);
    assert_eq!(changed.len(), 3);
    assert_eq!(changed[0].before.as_deref(), Some(a.as_str()));
    assert_eq!(changed[0].after.as_deref(), Some(b.as_str()));
    assert!(changed[1].before.is_none());
    assert!(changed[2].after.is_none());
    assert!(parse_refs("refs/heads/main\0not-a-hash\n").is_err());
    Ok(())
}
#[test]
fn result_checks_do_not_claim_unverified_cleanliness_and_hide_rule_text() {
    assert!(!check_result("search".into(), Err("tool failed".into())).passed);
    assert!(!check_result("search".into(), Ok("some history".into())).passed);
    assert!(check_result("search".into(), Ok("".into())).passed);
    let config = Config {
        replacements: vec![Replacement {
            find: "secret-value".into(),
            replace: "removed".into(),
        }],
        ..Default::default()
    };
    assert!(
        !config
            .redact("secret-value removed")
            .contains("secret-value")
    );
}
