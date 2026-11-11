use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_patch_menu_hover_resolves_explanation_in_both_locales() {
    for (language, label, explanation) in [
        (
            "en-US",
            "Apply patch…",
            "A patch from git format-patch becomes commits (git am), any other diff goes on the working tree (git apply)",
        ),
        (
            "de",
            "Patch anwenden…",
            "Ein Patch aus git format-patch wird zu Commits (git am), jeder andere Diff wird im Arbeitsverzeichnis angewendet (git apply)",
        ),
    ] {
        crate::i18n::set_language(language);
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (bool, Vec<Action>)| {
                if !std::mem::replace(&mut state.0, true) {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    return;
                }
                apply_patch_item(ui, true, &mut state.1);
                crate::a11y::end_frame(ui.ctx());
            },
            (false, Vec::new()),
        );
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::Button, label)
            .hover();
        h.run_steps(60);
        h.get_by_label(explanation);
        assert!(h.query_by_label(super::super::APPLY_PATCH_TIP).is_none());
        assert!(h.state().1.is_empty());
        h.get_by_role_and_label(egui::accesskit::Role::Button, label)
            .click();
        h.run();
        assert!(matches!(h.state().1.as_slice(), [Action::ApplyPatch]));
    }
    crate::i18n::set_language("en-US");
}
