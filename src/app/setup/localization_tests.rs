//! Inert production paint: no App, setup workers, account checks or file dialogs are started.
use super::*;
use crate::portability::{ConnectionRequirements, Identity, Provider, PublicAccountExpectation};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn author_rule_syntax_advisory_preserves_ports_ipv6_and_nested_namespaces() {
    for rule in [
        "github.com:team/x",
        "github.com//team",
        "github.com/team?x",
        "github.com/team#x",
    ] {
        assert!(author_rule_has_suspicious_syntax(rule), "{rule}");
    }
    for rule in [
        "",
        "custom-host/team",
        "github.com/",
        "github.com////",
        "custom-host/team////",
        "github.com:443/team",
        "[2001:db8::1]:8443/team",
        "2001:db8::1/team",
        "forge.example/team/https://nested",
    ] {
        assert!(!author_rule_has_suspicious_syntax(rule), "{rule}");
    }
}

#[test]
fn author_rule_url_advisory_preserves_supported_host_syntax() {
    for host in [
        "https://github.com/team",
        " SSH://forge.example/team ",
        "custom+scheme.1://forge.example/team",
    ] {
        assert!(author_rule_has_url_scheme(host), "{host}");
    }
    for host in [
        "",
        "   ",
        "github.com",
        "forge.example:8443/team/subteam",
        "[2001:db8::1]:8443/team",
        "forge.example/team/https://nested",
        "1invalid://forge.example",
    ] {
        assert!(!author_rule_has_url_scheme(host), "{host}");
    }
}

#[test]
fn setup_preview_warns_about_url_author_rule_without_changing_import_or_export()
-> Result<(), String> {
    crate::i18n::set_language("en-US");
    let mut settings = crate::settings::Settings::default();
    settings.user_profiles[0].author_hosts = vec![crate::settings::profiles::HostAuthor {
        host: "https://github.com/team".into(),
        name: "Team author".into(),
        email: "team@example.invalid".into(),
    }];
    settings.user_profiles[0]
        .author_hosts
        .push(crate::settings::profiles::HostAuthor {
            host: "github.com:team/x".into(),
            name: "Imported author".into(),
            email: "imported@example.invalid".into(),
        });
    let file = Setup::decode(&Setup::new(&settings, &BTreeMap::new()).encode()?)?;
    let before = file.encode()?;
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
    h.get_by_label("Author rule “https://github.com/team” includes a URL scheme and will not match. Use host or host/namespace, for example github.com/team. Review it in profile settings.");
    h.get_by_label("Author rule “github.com:team/x” may not match the intended remote. Use host/namespace, for example github.com/team/x; keep a numeric port after the host when needed. Review it in profile settings.");
    assert_eq!(h.state().encode()?, before);
    crate::i18n::set_language("de");
    h.run();
    h.get_by_label("Die Autorenregel „https://github.com/team“ enthält ein URL-Schema und wird nicht angewendet. Verwende Host oder Host/Namensraum, zum Beispiel github.com/team. Prüfe sie in den Profileinstellungen.");
    h.get_by_label("Die Autorenregel „github.com:team/x“ passt möglicherweise nicht zum gewünschten Remote. Verwende Host/Namensraum, zum Beispiel github.com/team/x; behalte bei Bedarf einen numerischen Port hinter dem Host bei. Prüfe sie in den Profileinstellungen.");
    assert_eq!(h.state().encode()?, before);
    Ok(())
}

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
