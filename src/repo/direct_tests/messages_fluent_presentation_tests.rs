use super::*;

#[test]
fn repository_operation_paint_preserves_control_labels_and_opaque_arguments() {
    let path = "src/Commit 日本語.rs";
    let raw = format!("Stage 1234 lines of {path}");
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        // Unknown external text forces all finite regexes to initialize, even when
        // a known German label could return early through the shared adapter.
        assert_eq!(
            display_operation("external-custom --flag=literal"),
            "external-custom --flag=literal"
        );
        let german = language == "de";
        assert_eq!(
            display_operation("Remove unused fork remotes (1234)"),
            if german {
                "Unbenutzte Fork-Remotes entfernen (1.234)"
            } else {
                "Remove unused fork remotes (1,234)"
            }
        );
        assert_eq!(
            display_operation("Abort rebase"),
            if german {
                "Rebase abbrechen"
            } else {
                "Abort rebase"
            }
        );
        assert_eq!(
            display_operation("Continue merge"),
            if german {
                "Merge fortsetzen"
            } else {
                "Continue merge"
            }
        );
        let rendered = display_operation(&raw);
        assert_eq!(
            rendered,
            if german {
                format!("1.234 Zeilen von {path} bereitstellen")
            } else {
                format!("Stage 1,234 lines of {path}")
            }
        );
        assert_eq!(
            display_operation("Stage 1 line of x"),
            if german {
                "1 Zeile von x bereitstellen"
            } else {
                "Stage 1 line of x"
            }
        );
        assert_eq!(
            display_operation("Fetch notes “Commit” from origin"),
            if german {
                "Notizen „Commit“ von origin abrufen"
            } else {
                "Fetch notes “Commit” from origin"
            }
        );
        assert_eq!(
            display_operation("Move aaaaaaaa above bbbbbbbb"),
            if german {
                "aaaaaaaa vor bbbbbbbb verschieben"
            } else {
                "Move aaaaaaaa above bbbbbbbb"
            }
        );
        assert_eq!(
            display_operation("Undo Sign off the commits of Commit/ü"),
            if german {
                "Rückgängig: Commits von Commit/ü mit Signed-off-by versehen"
            } else {
                "Undo Sign off the commits of Commit/ü"
            }
        );
        for custom in [
            "deploy --target=de",
            "Abort deployment",
            "Continue deployment",
            "Undo deployment script",
            "custom command with arguments",
            "外部コマンド",
        ] {
            assert_eq!(display_operation(custom), custom);
            assert_eq!(display_status(custom), custom);
        }
        assert!(raw.starts_with("Stage "));
        assert_eq!(raw, format!("Stage 1234 lines of {path}"));
        assert_eq!(
            crate::repo::pull::PullMode::FastForwardOnly.args(),
            &["pull", "--ff-only"]
        );
    }
    crate::i18n::set_language("xx-unsupported");
    assert_eq!(display_operation("Abort rebase"), "Abort rebase");
    assert_eq!(
        crate::i18n::text("repo-core-nonexistent-fallback-test"),
        "repo-core-nonexistent-fallback-test"
    );
}

#[test]
fn common_paint_uses_preserved_workflow_ids_and_raw_named_bindings() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let german = language == "de";
        assert_eq!(
            display_operation("external custom --opaque"),
            "external custom --opaque"
        );
        assert_eq!(
            display_status("File history cancelled"),
            if german {
                "Dateiverlauf abgebrochen"
            } else {
                "File history cancelled"
            }
        );
        assert_eq!(
            display_status("Summary cancelled"),
            if german {
                "Zusammenfassung abgebrochen"
            } else {
                "Summary cancelled"
            }
        );
        assert_eq!(
            display_operation("1234 paths"),
            if german { "1.234 Pfade" } else { "1,234 paths" }
        );
        assert_eq!(
            display_status("Push: wait for Fetch"),
            if german {
                "Pushen: Auf Abrufen warten"
            } else {
                "Push: wait for Fetch"
            }
        );
        let branch = "Commit/日本語";
        assert_eq!(
            display_operation(&format!("Push the signed-off commits of {branch}")),
            if german {
                format!("Signierte Commits von {branch} pushen")
            } else {
                format!("Push the signed-off commits of {branch}")
            }
        );
        assert_eq!(
            display_operation("custom deployment: wait for arbitrary script"),
            "custom deployment: wait for arbitrary script"
        );
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn repository_status_paint_localizes_only_known_errors_and_preserves_raw_state() {
    crate::i18n::set_language("de");
    let raw = "Analysis cancelled";
    assert_eq!(display_status(raw), crate::git::display_error(raw));
    assert_ne!(display_status(raw), raw);
    assert_eq!(
        display_status("fatal: external provider message in English"),
        "fatal: external provider message in English"
    );
    assert_eq!(display_status(crate::git::CANCELLED), "Abgebrochen");
    assert_eq!(raw, "Analysis cancelled");
    crate::i18n::set_language("en-US");
    assert_eq!(display_status(crate::git::CANCELLED), crate::git::CANCELLED);
}
