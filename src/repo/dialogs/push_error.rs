//! Paint-only push refusal. The retained raw error and any receipt stay untouched.
use super::Outcome;

pub(super) fn show(ui: &mut egui::Ui, raw: &str) -> Outcome {
    ui.heading(crate::i18n::text("repo-core-1396"));
    ui.label(crate::push_preflight::display::diagnostic(raw));
    if ui.button(crate::i18n::text("repo-core-0595")).clicked() {
        Outcome::Cancel
    } else {
        Outcome::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_push_refusal_paint_switches_language_and_only_close_cancels() {
        let raw = "Choose a remote before pushing this branch.";
        for (language, heading, error, close) in [
            ("en-US", "Push preflight could not finish", raw, "Close"),
            (
                "de",
                "Push-Vorabprüfung konnte nicht abgeschlossen werden",
                "Wähle vor dem Push dieses Branches ein Remote.",
                "Schließen",
            ),
        ] {
            crate::i18n::set_language(language);
            let mut h = Harness::new_ui_state(
                |ui, state: &mut (bool, String, bool)| {
                    if !std::mem::replace(&mut state.0, true) {
                        crate::theme::install_fonts(ui.ctx());
                        return;
                    }
                    if matches!(show(ui, &state.1), Outcome::Cancel) {
                        state.2 = true;
                    }
                },
                (false, raw.to_owned(), false),
            );
            h.run();
            h.get_by_label(heading);
            h.get_by_label(error);
            assert!(!h.state().2);
            assert_eq!(h.state().1, raw);
            h.get_by_role_and_label(egui::accesskit::Role::Button, close)
                .click();
            h.run();
            assert!(h.state().2);
            assert_eq!(h.state().1, raw);
        }
        crate::i18n::set_language("en-US");
    }

    #[test]
    fn actual_push_refusal_paint_keeps_unknown_detail_and_one_shot_receipt_opaque() {
        let opaque = "provider text: EXPECT APPROVED { $detail } 12345";
        let endpoint = "https://example.invalid/opaque-12345/repo";
        let receipt = crate::push_preflight::remember_auth_endpoint(endpoint);
        crate::i18n::set_language("de");
        for raw in [opaque, receipt.as_str()] {
            let mut h = Harness::new_ui_state(
                |ui, initialized: &mut bool| {
                    if !std::mem::replace(initialized, true) {
                        crate::theme::install_fonts(ui.ctx());
                        return;
                    }
                    assert!(matches!(show(ui, raw), Outcome::Open));
                },
                false,
            );
            h.run();
            h.get_by_label(raw);
        }
        // Painting never classifies, consumes or rebinds the one-shot route proof.
        assert_eq!(
            crate::push_preflight::take_auth_endpoint(&receipt).as_deref(),
            Some(endpoint)
        );
        assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
        crate::i18n::set_language("en-US");
    }
}
