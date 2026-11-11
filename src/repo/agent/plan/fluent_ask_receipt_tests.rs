use super::*;
use crate::repo::agent::CardView;
use egui_kittest::{Harness, kittest::Queryable};
use std::time::Duration;

#[test]
fn fluent_partition_actual_cached_approval_asks_repaint_with_raw_equality_and_source_guards() {
    let raw_name = "raw/ß; Exact source guards: not-a-delimiter\n日本語";
    let kinds = vec![
        AskPresentation::Stage {
            conflicted: vec![raw_name.into(), "second/path".into()],
        },
        AskPresentation::HardReset {
            target: raw_name.into(),
            from: "01234567".into(),
            to: "89abcdef".into(),
            subject: "opaque subject { $value }".into(),
            lost: 1234,
            left: 1234,
        },
        AskPresentation::MixedReset {
            target: raw_name.into(),
            from: "01234567".into(),
            to: "89abcdef".into(),
            subject: "opaque subject { $value }".into(),
            files: 1234,
        },
        AskPresentation::Drop {
            commit: "89abcdef".into(),
            subject: "opaque subject { $value }".into(),
            branch: raw_name.into(),
            after: 1234,
        },
        AskPresentation::DeleteBranch {
            name: raw_name.into(),
            commit: "89abcdef".into(),
            subject: "opaque subject { $value }".into(),
            force: true,
        },
        AskPresentation::Push {
            branch: raw_name.into(),
            remote: "fork/raw".into(),
            there: "target/ß".into(),
            publish: false,
            force: true,
            ahead: 1234,
            behind: 1234,
        },
        AskPresentation::Fallback {
            label: "Delete branch feature/x".into(),
        },
    ];
    let suffix = "\nExact source guards: raw-digest\n[[\"\\0expect-ref\", \"refs/heads/ß; Exact source guards: \\n\"]]";
    for kind in kinds {
        crate::i18n::set_language("en-US");
        let mut ask = Ask::from_presentation(kind.clone());
        let raw_before = ask.clone();
        let mut without_receipt = ask.clone();
        without_receipt.presentation = None;
        assert_eq!(
            ask, without_receipt,
            "presentation metadata never participates in replan equality"
        );
        ask.append_source_suffix(suffix);
        assert_eq!(ask.body, format!("{}{suffix}", raw_before.body));
        let card=CardView {agent:"opaque-agent".into(),preview:None,ask,commands:vec!["git push --force-with-lease=refs/heads/raw:01234567 fork refs/heads/raw:refs/heads/there".into()],undoable:false,left:Duration::from_secs(100),shown_for:Duration::from_secs(2)};
        let retained = card.clone();
        let mut installed = false;
        let mut h = Harness::new_ui_state(
            |ui, cached: &mut CardView| {
                if !installed {
                    crate::theme::install_fonts(ui.ctx());
                    installed = true;
                    ui.ctx().request_discard("install supplied fonts");
                    return;
                }
                let shown = cached.clone().for_display();
                ui.label(&shown.ask.title);
                ui.label(&shown.ask.body);
                ui.label(&shown.ask.undo_note);
            },
            card,
        );
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            h.run();
            let shown = h.state().clone().for_display();
            for value in [&shown.ask.title, &shown.ask.body, &shown.ask.undo_note] {
                assert!(h.get_all_by_label(value).count() >= 1);
            }
            assert_eq!(h.state(), &retained);
            assert_eq!(h.state().ask.body, retained.ask.body);
            assert_eq!(h.state().ask.title, raw_before.title);
            assert_eq!(h.state().ask.undo_note, raw_before.undo_note);
            assert_eq!(shown.commands, retained.commands);
            assert_eq!(shown.undoable, retained.undoable);
            assert_eq!(shown.left, retained.left);
            assert_eq!(shown.shown_for, retained.shown_for);
            assert!(shown.ask.body.ends_with(suffix));
            assert_presentation(&kind, &shown.ask, &retained.ask, language, raw_name, suffix);
            let mut changed = retained.ask.clone();
            changed.body.push_str("changed raw guard");
            assert_ne!(changed, retained.ask);
            assert_eq!(changed.for_display().body, changed.body);
        }
    }
    assert_push_count_variants();
    crate::i18n::set_language("en-US");
}

fn assert_presentation(
    kind: &AskPresentation,
    shown: &Ask,
    retained: &Ask,
    language: &str,
    raw_name: &str,
    suffix: &str,
) {
    match kind {
        AskPresentation::Fallback { label } => {
            assert_eq!(&shown.title, label);
            assert_eq!(shown.body, format!("{label}{suffix}"));
            assert_eq!(
                shown.undo_note,
                if language == "de" {
                    "Dies lässt sich in gitcraft nicht rückgängig machen."
                } else {
                    RAW_NO_UNDO
                }
            );
        }
        _ => {
            assert!(shown.body.contains(raw_name));
            if language == "de" {
                assert_ne!(shown.body, retained.body);
            }
        }
    }
    if matches!(
        kind,
        AskPresentation::HardReset { .. }
            | AskPresentation::MixedReset { .. }
            | AskPresentation::Drop { .. }
            | AskPresentation::Push { .. }
    ) {
        assert!(
            shown
                .body
                .contains(if language == "de" { "1.234" } else { "1,234" })
        );
    }
}

fn assert_push_count_variants() {
    for count in [0, 1, 1234] {
        for force in [false, true] {
            for publish in [false, true] {
                let kind = AskPresentation::Push {
                    branch: "feature/raw".into(),
                    remote: "fork".into(),
                    there: "target".into(),
                    publish,
                    force,
                    ahead: count,
                    behind: count,
                };
                let raw = kind.raw();
                for language in ["en-US", "de", "en-US"] {
                    crate::i18n::set_language(language);
                    let ask = Ask::from_presentation(kind.clone());
                    assert_eq!(ask, raw);
                    assert_eq!(ask.for_display().body, kind.display().body);
                }
            }
        }
    }
}
