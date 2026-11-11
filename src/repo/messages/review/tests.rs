mod fixed_cases;
use super::*;
use crate::repo::messages::{display_operation, display_status};

#[test]
fn review_finite_map_all_entries_render_in_both_locales_without_mutating_raw() {
    let fixed_expected = fixed_cases::ROWS;

    assert_eq!(fixed_expected.len(), FIXED.len());
    let dynamic_expected: &[(&str, &str, &str)] = &[
        (
            "Choose the repository for OPAQUE-component-Stage1/日本語/00001234",
            "Choose the repository for OPAQUE-component-Stage1/日本語/00001234",
            "Wähle das Repository für OPAQUE-component-Stage1/日本語/00001234",
        ),
        (
            "OPAQUE-component-Stage1/日本語/00001234 needs 2–4 ordered stages: deployed, optional bridge, next",
            "OPAQUE-component-Stage1/日本語/00001234 needs 2–4 ordered stages: deployed, optional bridge, next",
            "OPAQUE-component-Stage1/日本語/00001234 benötigt 2–4 geordnete Stufen: ausgeliefert, optionaler Übergang, nächste",
        ),
        (
            "Hunk refused: OPAQUE-detail-Stage1/日本語/00001234",
            "Hunk refused: OPAQUE-detail-Stage1/日本語/00001234",
            "Hunk abgelehnt: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Commit OPAQUE-commit-Stage1/日本語/00001234",
            "Commit OPAQUE-commit-Stage1/日本語/00001234",
            "Commit OPAQUE-commit-Stage1/日本語/00001234",
        ),
        (
            "Comparison OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Comparison OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Vergleich OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
        ),
        (
            "Pull request #OPAQUE-number-Stage1/日本語/00001234 · OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Pull request #OPAQUE-number-Stage1/日本語/00001234 · OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Pull Request #OPAQUE-number-Stage1/日本語/00001234 · OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
        ),
        (
            "Invalid rulebook from model: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid rulebook from model: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiges Regelwerk vom Modell: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Reviewer echo cannot infer push option OPAQUE-option-Stage1/日本語/00001234; review this operation explicitly",
            "Reviewer echo cannot infer push option OPAQUE-option-Stage1/日本語/00001234; review this operation explicitly",
            "Das Reviewer-Echo kann die Push-Option OPAQUE-option-Stage1/日本語/00001234 nicht auswerten; prüfe diese Operation ausdrücklich",
        ),
        (
            "Push mode OPAQUE-mode-Stage1/日本語/00001234 has no inferable source; review it explicitly",
            "Push mode OPAQUE-mode-Stage1/日本語/00001234 has no inferable source; review it explicitly",
            "Der Push-Modus OPAQUE-mode-Stage1/日本語/00001234 hat keine ableitbare Quelle; prüfe ihn ausdrücklich",
        ),
        (
            "Push source OPAQUE-source-Stage1/日本語/00001234 has no inspectable commit tree",
            "Push source OPAQUE-source-Stage1/日本語/00001234 has no inspectable commit tree",
            "Die Push-Quelle OPAQUE-source-Stage1/日本語/00001234 hat keinen untersuchbaren Commit-Baum",
        ),
        (
            "Review receipt: OPAQUE-detail-Stage1/日本語/00001234",
            "Review receipt: OPAQUE-detail-Stage1/日本語/00001234",
            "Review-Nachweis: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Invalid intent JSON: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid intent JSON: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiges Absichts-JSON: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Intent blob exceeds the 1234-byte budget",
            "Intent blob exceeds the 1,234-byte budget",
            "Der Absichts-Blob überschreitet das Limit von 1.234 Byte",
        ),
        (
            "Latest independent cleanup OPAQUE-name-Stage1/日本語/00001234 (OPAQUE-id-Stage1/日本語/00001234) · OPAQUE-detail-Stage1/日本語/00001234",
            "Latest independent cleanup OPAQUE-name-Stage1/日本語/00001234 (OPAQUE-id-Stage1/日本語/00001234) · OPAQUE-detail-Stage1/日本語/00001234",
            "Letzte unabhängige Bereinigung OPAQUE-name-Stage1/日本語/00001234 (OPAQUE-id-Stage1/日本語/00001234) · OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Container ownership inspection failed: OPAQUE-detail-Stage1/日本語/00001234",
            "Container ownership inspection failed: OPAQUE-detail-Stage1/日本語/00001234",
            "Prüfung des Container-Eigentümers fehlgeschlagen: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Cleanup needs attention: OPAQUE-detail-Stage1/日本語/00001234",
            "Cleanup needs attention: OPAQUE-detail-Stage1/日本語/00001234",
            "Bereinigung erfordert Aufmerksamkeit: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Could not preserve reproducer cleanup evidence for OPAQUE-name-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234; OPAQUE-result-Stage1/日本語/00001234",
            "Could not preserve reproducer cleanup evidence for OPAQUE-name-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234; OPAQUE-result-Stage1/日本語/00001234",
            "Bereinigungsnachweis für Reproduktionsfall OPAQUE-name-Stage1/日本語/00001234 konnte nicht gesichert werden: OPAQUE-detail-Stage1/日本語/00001234; OPAQUE-result-Stage1/日本語/00001234",
        ),
        (
            "Sandbox did not complete: OPAQUE-detail-Stage1/日本語/00001234",
            "Sandbox did not complete: OPAQUE-detail-Stage1/日本語/00001234",
            "Sandbox-Lauf nicht abgeschlossen: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Invalid preserved reproducer evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid preserved reproducer evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiger gesicherter Reproduktionsnachweis: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Submodule OPAQUE-path-Stage1/日本語/00001234 at 0123456789abcdef0123456789abcdef01234567: OPAQUE-detail-Stage1/日本語/00001234. Its pinned objects must already be present locally; gitcraft never clones or checks it out.",
            "Submodule OPAQUE-path-Stage1/日本語/00001234 at 0123456789abcdef0123456789abcdef01234567: OPAQUE-detail-Stage1/日本語/00001234. Its pinned objects must already be present locally; gitcraft never clones or checks it out.",
            "Submodul OPAQUE-path-Stage1/日本語/00001234 bei 0123456789abcdef0123456789abcdef01234567: OPAQUE-detail-Stage1/日本語/00001234. Seine fixierten Objekte müssen bereits lokal vorhanden sein; gitcraft klont es niemals und führt keinen Checkout aus.",
        ),
        (
            "Unsafe or unsupported sandbox source path: OPAQUE-path-Stage1/日本語/00001234",
            "Unsafe or unsupported sandbox source path: OPAQUE-path-Stage1/日本語/00001234",
            "Unsicherer oder nicht unterstützter Sandbox-Quellpfad: OPAQUE-path-Stage1/日本語/00001234",
        ),
        (
            "Unsafe symlink OPAQUE-path-Stage1/日本語/00001234",
            "Unsafe symlink OPAQUE-path-Stage1/日本語/00001234",
            "Unsicherer Symlink OPAQUE-path-Stage1/日本語/00001234",
        ),
        (
            "Symlink OPAQUE-path-Stage1/日本語/00001234 escapes the source root",
            "Symlink OPAQUE-path-Stage1/日本語/00001234 escapes the source root",
            "Symlink OPAQUE-path-Stage1/日本語/00001234 verlässt das Quellverzeichnis",
        ),
        (
            "Archive member OPAQUE-path-Stage1/日本語/00001234 traverses a symlink",
            "Archive member OPAQUE-path-Stage1/日本語/00001234 traverses a symlink",
            "Archiveintrag OPAQUE-path-Stage1/日本語/00001234 durchläuft einen Symlink",
        ),
        (
            "Invalid review evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid review evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiger Review-Nachweis: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Invalid shared storyboard: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid shared storyboard: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiges geteiltes Storyboard: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "1234 additional invalid records omitted",
            "1,234 additional invalid records omitted",
            "1.234 weitere ungültige Einträge ausgelassen",
        ),
    ];
    assert_eq!(dynamic_expected.len(), TEMPLATES.len());
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for &(source, en, de) in fixed_expected.iter().chain(dynamic_expected) {
            let stored = source.to_owned();
            let expected = if language == "de" { de } else { en };
            assert_eq!(display_operation(&stored), expected, "operation: {source}");
            assert_eq!(display_status(&stored), expected, "status: {source}");
            assert_eq!(stored, source);
        }
        assert_eq!(
            super::super::review_owned("1 additional invalid records omitted"),
            if language == "de" {
                "Ein weiterer ungültiger Eintrag ausgelassen"
            } else {
                "One additional invalid record omitted"
            }
        );
    }
}

#[test]
fn review_source_owned_diagnostics_keep_refs_commands_and_unknown_lookalikes_opaque() {
    let witnesses = [
        (
            "path/00001234.rs: Rust syntax not parsed (provider: Stage 1)",
            "path/00001234.rs: Rust-Syntax nicht geparst (provider: Stage 1)",
        ),
        (
            "src/a: Rust syntax not parsed (b.rs: Rust syntax not parsed (cannot parse string into token stream)",
            "src/a: Rust syntax not parsed (b.rs: Rust syntax not parsed (cannot parse string into token stream)",
        ),
        (
            "a → b: static call targets changed",
            "a → b: Ziele statischer Aufrufe geändert",
        ),
        (
            "a → b → module/01234 changed",
            "a → b → module/01234 geändert",
        ),
    ];
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for (raw, de) in witnesses {
            let stored = raw.to_owned();
            assert_eq!(dependency(&stored), if language == "de" { de } else { raw });
            assert_eq!(stored, raw);
        }
        // Source-derived review191: the path itself contains the old OID separator.
        for oid in [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789",
        ] {
            let path = "vendor/lib at stable";
            let detail = "Missing object";
            let raw = format!(
                "Submodule {path} at {oid}: {detail}. Its pinned objects must already be present locally; gitcraft never clones or checks it out."
            );
            let stored = raw.clone();
            let expected = if language == "de" {
                format!(
                    "Submodul {path} bei {oid}: {detail}. Seine fixierten Objekte müssen bereits lokal vorhanden sein; gitcraft klont es niemals und führt keinen Checkout aus."
                )
            } else {
                raw.clone()
            };
            assert_eq!(display_operation(&stored), expected);
            assert_eq!(display_status(&stored), expected);
            for field in [path, oid, detail] {
                assert!(expected.contains(field));
            }
            assert_eq!(stored, raw);
        }
        for oid in ["aaaaaaaa", "not-a-pinned-object"] {
            let raw = format!(
                "Submodule vendor/lib at stable at {oid}: Missing object. Its pinned objects must already be present locally; gitcraft never clones or checks it out."
            );
            assert_eq!(display_operation(&raw), raw);
            assert_eq!(display_status(&raw), raw);
        }
        let raw = "refs/gitcraft/intents/日本語/00001234: Invalid intent JSON: provider: Stage";
        assert_eq!(
            intent_warning(raw),
            if language == "de" {
                "refs/gitcraft/intents/日本語/00001234: Ungültiges Absichts-JSON: provider: Stage"
            } else {
                raw
            }
        );
        for detail in [
            "Stage",
            "Stage1",
            "Stage 1",
            "Abort custom-deploy",
            "Continue custom-script",
            "Undo Stage",
            "Preparing Stage",
            "provider: fatal: external diagnostic",
            "Docker sha256:0001234 · cargo test --offline --test gitcraft_reproducer",
        ] {
            let stored = format!("refs/gitcraft/intents/00001234: {detail}");
            assert_eq!(intent_warning(&stored), stored);
            assert_eq!(dependency(detail), detail);
            assert_eq!(super::super::review_owned(detail), detail);
        }
        for raw in [
            "Hunk refusedx: external",
            "invalid intent JSON: literal",
            "Stage1",
            "Stage 1",
            "Stage 0001",
            "Abort custom-deploy",
            "Continue custom-script",
            "provider: external prose",
            "No head run after cancellation",
            "Stopped after base; no head comparison was made",
            "Head source preparation did not complete",
            "unknown ID",
            "Original fix",
            "Adapted fix",
            "Configured when pinned (execution command not recorded by test lane): Stage",
            "refs/00001234: Stage",
        ] {
            // These strings are not eligible for the canonical review map. Dedicated
            // provenance adapters (or the pending compare seam) own their interpretation.
            assert_eq!(super::super::review_owned(raw), raw);
        }
        for raw in [
            "Abort custom-deploy",
            "Continue custom-script",
            "Stage1",
            "provider: external prose",
            "Original fix",
            "Adapted fix",
        ] {
            assert_eq!(display_operation(raw), raw);
            assert_eq!(display_status(raw), raw);
        }
    }
    crate::i18n::set_language("en-US");
}
