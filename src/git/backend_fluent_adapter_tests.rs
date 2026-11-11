use super::*;

#[test]
fn backend_fluent_operation_labels_keep_commands_and_control_identity() -> Result<(), String> {
    let opaque = "provider HTTP 409: /refs/heads/de/main";
    for (locale, merge, cancelled) in [
        ("en-US", "Merge", "Read cancelled"),
        ("de", "Zusammenführen", "Das Lesen wurde abgebrochen."),
    ] {
        crate::i18n::set_language(locale);
        assert_eq!(InProgress::Merge.label(), merge);
        for (op, command, continues) in [
            (InProgress::Merge, "merge", false),
            (InProgress::CherryPick, "cherry-pick", false),
            (InProgress::Revert, "revert", false),
            (InProgress::Rebase, "rebase", true),
            (InProgress::Am, "am", true),
        ] {
            assert_eq!(op.command(), command);
            assert_eq!(op.continues(), continues);
        }
        // Exercise the real production early-cancellation adapter: no command is built.
        let raw = command::draft_bytes(
            std::path::Path::new("/not-used"),
            &["status"],
            &[],
            Vec::new(),
            &[0],
            &|| false,
        )
        .err()
        .ok_or("missing cancellation failure")?;
        assert_eq!(raw, "Read cancelled");
        assert!(raw.contains("cancelled"));
        assert_eq!(display_error(&raw), cancelled);
        assert_eq!(display_error(opaque), opaque);
        for unknown in [
            "provider isn't a folder on opaque remote",
            "provider didn't finish within 3 seconds and was stopped",
        ] {
            assert_eq!(display_error(unknown), unknown);
        }
        let helper = "Contained bundle Git: invalid keep hash";
        assert_eq!(
            display_error(helper),
            if locale == "de" {
                "Git im begrenzten Bundle-Helfer: Ungültiger Keep-Hash."
            } else {
                helper
            }
        );
        let steps = dvc::pull_steps();
        assert_eq!(
            steps[0].last().map(String::as_str),
            Some("gitcraft-dvc-pull")
        );
        assert!(steps[0][1].contains("DVC is not installed on this repository machine"));
        assert!(goes_over_network(
            &steps[0].iter().map(String::as_str).collect::<Vec<_>>()
        ));
        assert_eq!(testlane::Verdict::Good.word(), "good");
        assert_eq!(
            testlane::Verdict::parse("bad"),
            Some(testlane::Verdict::Bad)
        );
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn backend_fluent_supplied_state_keeps_argv_refs_and_payloads() -> Result<(), String> {
    use doctor::health::{Health, Objects, suggestions};
    let health = Health {
        status_took: Some(std::time::Duration::from_millis(1234)),
        remote: true,
        objects: Objects {
            loose: 2345,
            packs: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let query = "author:\"Jane Doe\" after:2w path:src/de.rs";
    crate::i18n::set_language("en-US");
    let english_plan = search_query::prepare(query, false).map_err(|e| e.message)?;
    let english_ops: Vec<_> = suggestions(&health).into_iter().map(|s| s.ops).collect();
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        let cmd = command::apply_command_extras(
            std::process::Command::new("/fixed/git-not-executed"),
            None,
            &command::Extra::default(),
            &[
                "fetch",
                "origin",
                "refs/heads/de/main:refs/remotes/origin/de/main",
            ],
        );
        assert_eq!(cmd.get_program(), "/fixed/git-not-executed");
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            [
                "fetch",
                "origin",
                "refs/heads/de/main:refs/remotes/origin/de/main"
            ]
        );
        assert_eq!(
            search_query::prepare(query, false).map_err(|e| e.message)?,
            english_plan
        );
        assert_eq!(
            suggestions(&health)
                .into_iter()
                .map(|s| s.ops)
                .collect::<Vec<_>>(),
            english_ops
        );
        let warning = suggestions(&health)[2].why.clone();
        assert!(
            warning.contains(if locale == "de" {
                "1.234 ms"
            } else {
                "1,234 ms"
            }),
            "{warning}"
        );
        assert_eq!(parse_lfs_pointer("version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 1234\n").map(|p| p.size), Some(1234));
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn backend_fluent_file_and_line_labels_use_numeric_counts() -> Result<(), String> {
    use file_comparison::Version;
    for (locale, one, many, binary, lines, size) in [
        (
            "en-US",
            "100644 · 1 byte",
            "100644 · 1,234 bytes",
            "100644 · 2 bytes · binary",
            "lines 1,234–2,345",
            "1.0 KB",
        ),
        (
            "de",
            "100644 · 1 Byte",
            "100644 · 1.234 Bytes",
            "100644 · 2 Bytes · binär",
            "Zeilen 1.234–2.345",
            "1,0 KB",
        ),
    ] {
        crate::i18n::set_language(locale);
        let make = |bytes| Version {
            mode: Some("100644".into()),
            oid: Some("a".repeat(40)),
            bytes,
            warning: None,
        };
        assert_eq!(make(vec![1]).label(), one);
        assert_eq!(make(vec![1; 1234]).label(), many);
        assert_eq!(make(vec![0, 1]).label(), binary);
        assert_eq!(
            crate::forge::links::Lines::new(1234, 2345)
                .ok_or("missing line range")?
                .label(),
            lines
        );
        assert_eq!(human_size(1024), size);
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
