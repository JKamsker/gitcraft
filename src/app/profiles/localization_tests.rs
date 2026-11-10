use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_profile_row_keeps_identity_selection_and_opaque_author_across_languages()
-> Result<(), String> {
    crate::i18n::set_language("de");
    let profile = settings::UserProfile {
        name: "Arbeitsprofil 日本語".into(),
        author_name: "Jonas 日本語".into(),
        author_email: "jonas@example.invalid".into(),
        ..Default::default()
    };
    let before = serde_json::to_string(&profile).map_err(|error| format!("{error:?}"))?;
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, state: &mut (settings::UserProfile, bool, Option<egui::Id>)| {
            if !fonts.replace(true) {
                theme::install_fonts(ui.ctx());
                return;
            }
            let response = profile_row(ui, &state.0, true, &Tokens::get(ui.ctx()));
            if let Some(id) = state.2 {
                assert_eq!(response.id, id);
            }
            state.2 = Some(response.id);
            state.1 |= response.clicked();
        },
        (profile, false, None),
    );
    h.run();
    h.get_by_label("Profil Arbeitsprofil 日本語").click();
    h.run();
    assert!(h.state().1);
    assert_eq!(
        author_line(&h.state().0),
        " · Jonas 日本語 <jonas@example.invalid>"
    );
    assert_eq!(
        serde_json::to_string(&h.state().0).map_err(|error| format!("{error:?}"))?,
        before
    );
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("profile Arbeitsprofil 日本語");
    assert_eq!(
        serde_json::to_string(&h.state().0).map_err(|error| format!("{error:?}"))?,
        before
    );

    Ok(())
}

#[test]
fn unfinished_work_translates_only_known_display_phrases() {
    crate::i18n::set_language("de");
    assert_eq!(unfinished_phrase("running"), "einen laufenden Vorgang");
    assert_eq!(unfinished_phrase("terminal"), "ein geöffnetes Terminal");
    assert_eq!(
        unfinished_phrase("commit message"),
        "eine geschriebene Commit-Nachricht"
    );
    assert_eq!(unfinished_phrase("editor"), "einen geöffneten Editor");
    assert_eq!(unfinished_phrase("review draft"), "einen Review-Entwurf");
    assert_eq!(
        unfinished_phrase("external operation 日本語"),
        "external operation 日本語"
    );
    crate::i18n::set_language("en-US");
    assert_eq!(unfinished_phrase("running"), "an operation running");
}
