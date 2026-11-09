//! Inert production paint: no App, setup workers, account checks or file dialogs are started.
use super::*;
use crate::portability::{ConnectionRequirements, Identity, Provider, PublicAccountExpectation};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_nonempty_setup_preview_keeps_imported_fields_opaque_and_localizes_both_profile_states()
-> Result<(), String> {
    crate::i18n::set_language("de");
    let mut settings = crate::settings::Settings::default();
    let profile = &mut settings.user_profiles[0];
    profile.name = "literal $() 日本語".into();
    profile.author_name = "Autor {literal}".into();
    profile.author_email = "literal@example.invalid".into();
    profile.pending_setup = Some(ConnectionRequirements {
        ssh_key_reselection_required: true,
        signing_key_reselection_required: true,
        accounts: vec![PublicAccountExpectation {
            provider: Provider::Github,
            host: "forge.example.invalid".into(),
            user_or_alias: "literal-user".into(),
            account_user: None,
        }],
    });
    let mut file = Setup::new(&settings, &BTreeMap::new());
    let mut ready = file.profiles[0].clone();
    ready.name = "ready-profile".into();
    ready.setup = Default::default();
    file.profiles.push(ready);
    file.identities.resize(
        1234,
        Identity {
            label: "literal author".into(),
            name: "literal name".into(),
            email: "literal@invalid".into(),
        },
    );
    let before = serde_json::to_string(&file).map_err(|e| e.to_string())?;
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, file: &mut Setup| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            preview_summary(ui, file);
        },
        file,
    );
    h.run();
    h.get_by_label("Vorschau: 2 Profile, 1.234 gespeicherte Autoren, 0 Repositorys");
    h.get_by_label("Profil: literal $() 日本語 — Autor {literal} <literal@example.invalid> · lokale Authentifizierung und Schlüsseleinrichtung erforderlich");
    h.get_by_label("Profil: ready-profile — Autor {literal} <literal@example.invalid>");
    h.get_by_label("Github: literal-user auf forge.example.invalid");
    assert_eq!(
        serde_json::to_string(h.state()).map_err(|e| e.to_string())?,
        before
    );
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("Preview: 2 profiles, 1,234 saved authors, 0 repositories");
    h.get_by_label("Profile: literal $() 日本語 — Autor {literal} <literal@example.invalid> · local authentication/key setup required");
    assert_eq!(
        serde_json::to_string(h.state()).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}
