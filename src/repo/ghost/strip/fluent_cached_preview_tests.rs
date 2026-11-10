use super::*;
use egui_kittest::{Harness, kittest::Queryable};
#[test]
fn fluent_partition_actual_cached_ghost_preview_repaints_raw_notes_without_workers() {
    let notes = vec![
        "Uncommitted changes are stashed first and put back after (rebase.autoStash).".into(),
        "As of the last fetch of fork/ß: what was pushed there since isn't shown.".into(),
        "1234 commits are dropped: their changes are already there.".into(),
        "Provider opaque: 日本語\n{message} --argv=raw".into(),
    ];
    let warnings = vec![
        "Uncommitted changes to these files are thrown away, and can't be brought back.".into(),
    ];
    let raw_refused =
        "feature/ß and fork/branch have diverged, so a fast-forward-only pull refuses";
    let state = Outcome {
        notes: notes.clone(),
        warnings: warnings.clone(),
        refused: Some(raw_refused.into()),
        ..Default::default()
    };
    let mut initialized = false;
    let mut h = Harness::new_ui_state(
        |ui, state: &mut Outcome| {
            if !initialized {
                crate::theme::install_fonts(ui.ctx());
                initialized = true;
                ui.ctx().request_discard("install supplied fonts");
                return;
            }
            outcome_lines(ui, &Tokens::get(ui.ctx()), state);
        },
        state,
    );
    for lang in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(lang);
        h.run();
        for note in &notes {
            h.get_by_label(&super::super::plan::display_preview_text(note));
        }
        h.get_by_label(&super::super::plan::display_preview_text(&warnings[0]));
        assert!(
            super::super::plan::display_preview_text(&notes[2]).contains(if lang == "de" {
                "1.234"
            } else {
                "1,234"
            })
        );
        assert_eq!(h.state().notes, notes);
        assert_eq!(h.state().warnings, warnings);
        assert_eq!(h.state().refused.as_deref(), Some(raw_refused));
        assert_eq!(
            super::super::plan::display_preview_text(&notes[3]),
            notes[3]
        );
    }
}
