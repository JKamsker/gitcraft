use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn enter_on_remote_cancel_closes_without_submitting() {
    crate::i18n::set_language("en-US");
    let mut h = Harness::new_ui_state(
        |ui, state: &mut Option<App>| {
            if state.is_none() {
                let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                let mut app =
                    App::launched_using(&cc, None, || Ok("git version 2.43.0 (recorded)".into()));
                app.remote_form = Some(RemoteForm {
                    kind: Kind::Ssh,
                    distros: vec![],
                    distro: String::new(),
                    distros_rx: None,
                    hosts: vec![],
                    host: "-invalid".into(),
                    path: "/inert/repository".into(),
                    error: None,
                    focus: false,
                    browser: None,
                });
                *state = Some(app);
                return;
            }
            if let Some(app) = state {
                app.remote_dialog(ui.ctx());
            }
        },
        None,
    );
    h.run_steps(3);
    h.get_by_label("Cancel").focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(
        h.state()
            .as_ref()
            .is_some_and(|app| app.remote_form.is_none() && app.connecting.is_empty())
    );
}

#[test]
fn actual_remote_fields_switch_kind_with_opaque_targets_and_no_discovery() {
    crate::i18n::set_language("de");
    let form = RemoteForm {
        kind: Kind::Ssh,
        distros: vec!["Ubuntu-24.04".into()],
        distro: "Ubuntu-24.04".into(),
        distros_rx: None,
        hosts: vec![],
        host: "me@example.invalid".into(),
        path: "~/repo 日本語".into(),
        error: None,
        focus: false,
        browser: None,
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, form: &mut RemoteForm| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            assert!(!form_fields(ui, form, &Tokens::get(ui.ctx())).0);
        },
        form,
    );
    h.run();
    h.get_by_label("Ordner dort");
    h.get_by_label("Es werden Schlüssel und ein Agent verwendet; ein Passwort oder eine Passphrase wird hier abgefragt.");
    h.get_by_label("WSL").click();
    h.run();
    assert_eq!(h.state().kind, Kind::Wsl);
    assert_eq!(
        h.state().target(),
        Ok((Host::Wsl("Ubuntu-24.04".into()), "~/repo 日本語".into()))
    );
    h.get_by_label("SSH").click();
    h.run();
    assert_eq!(
        h.state().target(),
        Ok((
            Host::Ssh("me@example.invalid".into()),
            "~/repo 日本語".into()
        ))
    );
    assert!(h.state().distros_rx.is_none() && h.state().browser.is_none());
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("Folder there");
}

#[test]
fn remote_validation_and_status_are_display_only_and_keep_raw_machine_path_values() {
    crate::i18n::set_language("de");
    for (raw, expected) in [
        (
            "Pick a WSL distribution.",
            "Wähle eine WSL-Distribution aus.",
        ),
        (
            "The folder is a path on that machine: /home/me/repo or ~/repo.",
            "Der Ordner muss als Pfad auf dem anderen Rechner angegeben werden: /home/me/repo oder ~/repo.",
        ),
        (
            "The host is what ssh takes: an alias of ~/.ssh/config or user@host.",
            "Der Host wird wie bei ssh angegeben: ein Alias aus ~/.ssh/config oder user@host.",
        ),
        (
            "Connecting failed unexpectedly.",
            "Die Verbindung ist unerwartet fehlgeschlagen.",
        ),
        (
            "Connecting to ssh://me@example.invalid/日本語…",
            "Verbindung mit ssh://me@example.invalid/日本語 wird hergestellt…",
        ),
        (
            "could not find the home folder on SSH · me@example.invalid",
            "Der persönliche Ordner auf SSH · me@example.invalid konnte nicht ermittelt werden",
        ),
    ] {
        assert_eq!(display_message(raw).as_deref(), Some(expected));
    }
    assert!(display_message("External SSH diagnostic 日本語").is_none());
    crate::i18n::set_language("en-US");
}
