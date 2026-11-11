mod assertions;
use super::*;
use crate::repo::messages::{display_operation, display_status};

#[derive(Clone, Copy)]
enum Sample {
    Number(u64),
    Text(&'static str, &'static str),
}
#[derive(Clone, Copy)]
struct Case {
    raw: &'static str,
    id: &'static str,
    args: &'static [(&'static str, Sample)],
}
mod cases;
use cases::ROWS as CASES;

#[test]
fn workflow_finite_contracts_initialize_all_patterns_and_render_named_types_in_both_locales() {
    assert_eq!(CASES.len(), TEMPLATES.len());
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for case in CASES {
            let args: Vec<(&str, crate::i18n::Value<'_>)> = case
                .args
                .iter()
                .map(|(name, sample)| {
                    (
                        *name,
                        match sample {
                            Sample::Number(n) => (*n).into(),
                            Sample::Text(en, de) => {
                                (if language == "de" { *de } else { *en }).into()
                            }
                        },
                    )
                })
                .collect();
            let expected = crate::i18n::message(case.id, &args);
            assert_ne!(expected, case.id, "embedded catalog: {}", case.id);
            let stored = case.raw.to_owned();
            assert_eq!(
                display_operation(&stored),
                expected,
                "operation {}: {}",
                case.id,
                case.raw
            );
            assert_eq!(
                display_status(&stored),
                expected,
                "status {}: {}",
                case.id,
                case.raw
            );
            assert_eq!(stored, case.raw);
        }
    }
}

#[test]
fn workflow_selector_defaults_nested_guards_and_numeric_identity_are_paint_only() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let german = language == "de";
        for (raw, target, remote) in [
            (
                "refs/heads/00001234's commits are signed off here, but not pushed: Ctrl+Z puts them back as they were",
                "no",
                "",
            ),
            (
                "refs/heads/00001234's commits are signed off here, but not pushed to ssh://opaque/00001234: Ctrl+Z puts them back as they were",
                "yes",
                "ssh://opaque/00001234",
            ),
        ] {
            let expected = crate::i18n::message(
                "repo-workflows-status-dco-local-only",
                &[
                    ("branch", "refs/heads/00001234".into()),
                    ("target", target.into()),
                    ("remote", remote.into()),
                ],
            );
            let stored = raw.to_owned();
            assert_eq!(display_operation(&stored), expected);
            assert_eq!(display_status(&stored), expected);
            assert_eq!(stored, raw);
        }
        for (raw, kept, names) in [
            ("No fork remote can go", "no", ""),
            (
                "No fork remote can go\n\nKept: refs/00001234, 日本語, whose branches have commits no branch here has.",
                "yes",
                "refs/00001234, 日本語",
            ),
        ] {
            let expected = crate::i18n::message(
                "repo-workflows-status-forks-none-to-remove",
                &[("kept", kept.into()), ("names", names.into())],
            );
            assert_eq!(display_status(raw), expected);
        }
        // Source-derived workflow60: both splits are legal opaque value/file data.
        let raw = "Remove core.editor (old) from new) from .git/config";
        let stored = raw.to_owned();
        assert_eq!(display_operation(&stored), raw);
        assert_eq!(display_status(&stored), raw);
        assert_eq!(stored, raw);
        let pr = "Rebase #00001234 onto refs/heads/00001234 and push";
        let shown = display_operation(pr);
        assert!(shown.contains("00001234"));
        assert!(!shown.contains("1.234"));
        assert!(!shown.contains("1,234"));
        for raw in [
            "Force unlock 1234 files",
            "Distribute staged fixes into 1234 commits",
        ] {
            assert!(display_operation(raw).contains(if german { "1.234" } else { "1,234" }));
        }
        assertions::unknown_workflow_diagnostics();
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn workflow_receipt_adapter_accepts_only_finite_owned_clause_order_and_opaque_external_data() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for raw in [
            "Final files identical",
            "Final files differ, too many commits to follow one by one",
            "Final files identical, 1 author changed, 2 messages changed, 1234 commits now signed, 2 commits → 1234",
            "Final files identical, authors kept, messages kept, 1 signature dropped",
            "Final files differ, 2 authors changed, 1 message changed, signatures replaced",
            "Final files differ, signatures replaced, 12 dropped",
            "Final files differ, signatures replaced, 12 more signed",
        ] {
            let shown = receipt(raw);
            assert!(shown.is_some(), "receipt missing: {raw}");
        }
        for raw in [
            "external command",
            "Stage 1",
            "Final files identical, provider prose",
            "Final files identical, 2 author changed",
            "Final files identical, 1 authors changed",
            "Final files identical, authors kept, authors kept",
            "Final files identical, messages kept, authors kept",
            "Final files identical, too many commits to follow one by one, authors kept",
            "Final files differ, signatures replaced, arbitrary dropped",
            "Final files differ, signatures replaced, 0 dropped",
            "Final files differ, signatures replaced, 1,234 dropped",
        ] {
            assert_eq!(receipt(raw), None, "external/malformed receipt: {raw}");
        }
        let raw = "Push: done. Final files identical, authors kept, messages kept (receipt in the timeline, Ctrl+Shift+Z)";
        let rendered = display_status(raw);
        assert_eq!(
            raw,
            "Push: done. Final files identical, authors kept, messages kept (receipt in the timeline, Ctrl+Shift+Z)"
        );
        assert!(rendered.contains(if language == "de" {
            "Dateien am Ende identisch"
        } else {
            "Final files identical"
        }));
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn workflow_finite_scope_health_noun_and_state_choices_render_each_supplied_contract() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for (raw, id) in [
            (
                "this repository",
                "repo-workflows-doctor-scope-choice-38429e22",
            ),
            (
                "this worktree",
                "repo-workflows-doctor-scope-choice-0eb26325",
            ),
            (
                "all my repositories",
                "repo-workflows-doctor-scope-choice-59b8031f",
            ),
            (
                "everyone on this machine",
                "repo-workflows-doctor-scope-choice-ad944dcb",
            ),
            (
                "command line",
                "repo-workflows-doctor-scope-choice-c925a118",
            ),
        ] {
            let input = format!("Set opaque-key to opaque-value for {raw}");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-doctor-set-row-921f564b",
                    &[
                        ("setting-key-1", "opaque-key".into()),
                        ("shortened-value-2", "opaque-value".into()),
                        (
                            "scope-choice-scope-to-lowercase-3",
                            crate::i18n::text(id).into()
                        )
                    ]
                )
            );
        }
        for (raw, id) in [
            ("commit-graph", "backend-health-commit-graph"),
            (
                "scheduled maintenance",
                "backend-scheduled-maintenance-f688c9",
            ),
            ("file system monitor", "backend-file-system-monitor-bd6322"),
            ("untracked cache", "backend-untracked-cache-de0942"),
            ("pack the objects", "backend-pack-the-objects-5a853d"),
        ] {
            let input = format!("Turn on: {raw}");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-doctor-turn-on-action-d8e3e02d",
                    &[("s-title-to-lowercase-1", crate::i18n::text(id).into())]
                )
            );
        }
        for (raw, id) in [
            ("pull request", "backend-noun-pull-request"),
            ("merge request", "backend-noun-merge-request"),
        ] {
            let input = format!("Publish {raw}s");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-stack-start-publish-c4ffa0c1",
                    &[("noun", crate::i18n::text(id).into())]
                )
            );
        }
        for (raw, id) in [
            ("merge", "backend-in-progress-merge"),
            ("cherry-pick", "backend-in-progress-cherry-pick"),
            ("revert", "backend-in-progress-revert"),
            ("rebase", "backend-in-progress-rebase"),
            ("patch", "backend-in-progress-patch"),
        ] {
            let input = format!("Distribute staged fixes: a {raw} is in progress");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-surgery-open-absorb-edcc444b",
                    &[("state-label-to-lowercase-1", crate::i18n::text(id).into())]
                )
            );
        }
        for raw in ["this repository ", "THIS REPOSITORY", "custom scope"] {
            assert!(scope(raw).is_none());
        }
        for raw in [
            "commit-graph ",
            "Commit-graph",
            "external provider suggestion",
        ] {
            assert!(health(raw).is_none());
        }
        for raw in ["pull requests", "custom request"] {
            assert!(noun(raw).is_none());
        }
        for raw in ["Merge", "external state"] {
            assert!(in_progress(raw).is_none());
        }
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn workflow_checkout_contract_preserves_decimal_pr_and_opaque_owner_in_public_wrappers() {
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for number in ["1234", "00001234"] {
            for owner in [
                "raw-owner/ß",
                "Stage1 to push to Continue custom-script: 日本語/00001234",
                "$(cargo test) --offline",
            ] {
                let raw = format!("Check out #{number} to push to {owner}");
                let stored = raw.clone();
                let expected = if language == "de" {
                    format!("#{number} auschecken, um nach {owner} zu pushen")
                } else {
                    raw.clone()
                };
                assert_eq!(display_operation(&stored), expected);
                assert_eq!(display_status(&stored), expected);
                assert!(expected.contains(&format!("#{number}")));
                assert!(expected.contains(owner));
                assert!(!expected.contains("#1,234"));
                assert!(!expected.contains("#1.234"));
                assert_eq!(stored, raw);
                let nested = format!("Undo {raw}");
                let nested_expected = crate::i18n::message(
                    "interaction-operation-undo",
                    &[("operation", expected.into())],
                );
                assert_eq!(display_operation(&nested), nested_expected);
                assert_eq!(display_status(&nested), nested_expected);
            }
        }
        for raw in [
            "Check out #Stage1 to push to raw-owner/ß",
            "Check out #1,234 to push to raw-owner/ß",
            "Check out #1.234 to push to raw-owner/ß",
            "Check out #-1 to push to raw-owner/ß",
            "Check out #+1 to push to raw-owner/ß",
            "Check out #1234 to push toward raw-owner/ß",
            "custom Check out #1234 to push to raw-owner/ß",
            "Undo Check out #provider to push to raw-owner/ß",
            "Preparing Check out #provider to push to raw-owner/ß",
            "Continue custom-script Check out #1234 to push to raw-owner/ß",
            "Stage1",
        ] {
            let stored = raw.to_owned();
            assert_eq!(display_operation(&stored), raw);
            assert_eq!(display_status(&stored), raw);
            assert_eq!(stored, raw);
        }
    }
}

#[test]
fn workflow_r1_fixed_sentinels_and_raw_operation_templates_project_only_at_paint() {
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for (raw, id) in &[
            (
                "History worker stopped",
                "repo-workflows-saved_views-poll-da12ac51",
            ),
            (
                "History read cancelled",
                "repo-workflows-saved_views-poll-dc101692",
            ),
            (
                "History worker panicked",
                "repo-workflows-saved_views-poll-33647b3e",
            ),
            (
                "Use YYYY-MM-DD dates",
                "repo-workflows-atlas-read-atlas-02c42736",
            ),
            (
                "Use YYYY-MM-DD dates, with start on or before end",
                "repo-workflows-atlas-read-atlas-67b18cbf",
            ),
            (
                "Use dates such as 2026-11-07",
                "repo-workflows-history_activity-requested-range-4d18caf5",
            ),
            (
                "The start date must be on or before the end date",
                "repo-workflows-history_activity-requested-range-25f6acd0",
            ),
            (
                "Analysis cancelled. Refresh to try again.",
                "repo-workflows-history_activity-cancel-ddfe1c43",
            ),
            (
                "No supported forge remote; commit history remains available",
                "repo-workflows-line_dossier-poll-line-dossier-45e16671",
            ),
            (
                "Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history",
                "repo-workflows-line_dossier-poll-line-dossier-1cd553a8",
            ),
            (
                "Trace cancelled",
                "repo-workflows-line_dossier-line-dossier-view-a8198076",
            ),
            (
                "Repository changed. Refresh the file view.",
                "repo-workflows-file_comparison-invalidate-81f92ce0",
            ),
            (
                "Invalid comparison sides",
                "repo-workflows-file_comparison-prepare-da3978aa",
            ),
            (
                "More than 20,000 lines in a version; use the normal patch view",
                "repo-workflows-file_comparison-boundary-e44610d7",
            ),
            (
                "Git could not align these raw versions",
                "repo-workflows-file_comparison-boundary-b861cbb0",
            ),
            (
                "The raw patch is not UTF-8",
                "repo-workflows-file_comparison-boundary-5421cc1b",
            ),
            (
                "Git could not produce a raw text patch for these differing bytes; use the normal patch view",
                "repo-workflows-file_comparison-boundary-6732715f",
            ),
            (
                "Comparison cancelled. Refresh to try again.",
                "repo-workflows-file_comparison-file-comparison-view-725513de",
            ),
            (
                "No key, agent, account or network action has run. Choose each step explicitly.",
                "repo-workflows-ssh_setup-new-8633c172",
            ),
            (
                "Setup worker stopped; no later result will be applied.",
                "repo-workflows-ssh_setup-poll-380743c6",
            ),
        ] {
            assert_eq!(
                crate::repo::messages::display_status(raw),
                crate::i18n::text(id)
            );
        }
        for raw in [
            "Cut release tag/$(opaque)\n1234",
            "Clear the flag of path/{$x}: before\n1234",
            "Clear the flags of 1234 files",
        ] {
            let shown = crate::repo::messages::display_operation(raw);
            if language == "en-US" && !raw.ends_with(" files") {
                assert_eq!(shown, raw);
            }
            if language == "de" {
                assert_ne!(shown, raw);
            }
        }
        let private =
            "Binary or non-UTF8 versions: compare the byte preview; line staging is unavailable";
        assert_eq!(crate::repo::messages::display_status(private), private);
        assert_eq!(
            crate::repo::activity::display_workflow_error(private),
            private
        );
        for opaque in [
            "provider arbitrary diagnostic",
            "Cut release",
            "Clear the flags of 1.234 files",
        ] {
            assert_eq!(crate::repo::messages::display_operation(opaque), opaque);
        }
    }
}
