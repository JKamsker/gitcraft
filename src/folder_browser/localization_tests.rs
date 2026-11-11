use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_inert_browser_picks_original_path_and_keeps_raw_recorded_listing_failures()
-> Result<(), String> {
    crate::i18n::set_language("de");
    let browser = FolderBrowser {
        host: Host::Ssh("recorded-host".into()),
        listing: Some(Listing {
            dir: "/opaque/root".into(),
            repo: true,
            cut: true,
            folders: vec![
                Folder {
                    name: "日本語".into(),
                    repo: true,
                },
                Folder {
                    name: ".hidden".into(),
                    repo: false,
                },
            ],
        }),
        loading: None,
        error: None,
        selected: None,
        hidden: false,
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, state: &mut (FolderBrowser, Option<String>)| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            if let Some(Outcome::Picked(path)) = state.0.ui(ui) {
                state.1 = Some(path);
            }
        },
        (browser, None),
    );
    h.run();
    h.get_by_label("Dieser Ordner ist ein Git-Repository.");
    h.get_by_label("Nur die ersten 5.000 Ordner werden aufgelistet.");
    h.get_by_label("日本語, Repository").click();
    h.run();
    assert_eq!(h.state().0.selected, Some(0));
    h.get_by_label("Ordner auswählen").click();
    h.run();
    assert_eq!(h.state().1.as_deref(), Some("/opaque/root/日本語"));
    assert!(h.state().0.loading.is_none());
    assert_eq!(h.state().0.host, Host::Ssh("recorded-host".into()));
    h.get_by_label("Versteckte Ordner").click();
    h.run();
    assert!(h.state().0.hidden);
    h.get_by_label(".hidden, Ordner");
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("Select folder");
    h.get_by_label(".hidden, folder");
    assert_eq!(h.state().0.selected, Some(0));
    // Empty supplied receiver drives actual pending paint and completion routing without a worker.
    crate::i18n::set_language("de");
    let (tx, rx) = channel();
    let dir = "/opaque/name on SSH · recorded-host: literal".to_owned();
    h.state_mut().0.listing = None;
    h.state_mut().0.loading = Some((dir.clone(), rx));
    h.run_steps(3);
    h.get_by_label("Verbindung zu SSH · recorded-host wird hergestellt…");
    let raw = format!("There is no folder {dir} on SSH · recorded-host.");
    tx.send(Err(raw.clone())).map_err(|e| e.to_string())?;
    h.run();
    h.get_by_label(&format!(
        "Auf SSH · recorded-host gibt es keinen Ordner {dir}."
    ));
    assert_eq!(h.state().0.error, Some((dir.clone(), raw.clone())));
    let native_raw = "it is a symbolic link".to_owned();
    h.state_mut().0.error = Some((dir.clone(), native_raw.clone()));
    h.run();
    h.get_by_label("Dies ist ein symbolischer Link.");
    assert_eq!(h.state().0.error, Some((dir.clone(), native_raw.clone())));
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label(&native_raw);
    assert_eq!(h.state().0.error, Some((dir, native_raw)));
    Ok(())
}

#[test]
fn known_listing_envelopes_use_original_typed_directory_and_host_and_keep_unknown_details_opaque() {
    crate::i18n::set_language("de");
    let host = Host::Ssh("recorded-host".into());
    let dir = "/日本語 on SSH · recorded-host: literal";
    let detail = "external 日本語: Could not list anything\nunchanged";
    let raw = format!("Could not list {dir} on {}: {detail}", host.label());
    assert_eq!(
        display_list_error(&raw, dir, &host),
        format!(
            "Der Ordner {dir} auf SSH · recorded-host konnte nicht aufgelistet werden: {detail}"
        )
    );
    let malformed = format!("Could not list {dir} on {}.", host.label());
    assert_eq!(
        display_list_error(&malformed, dir, &host),
        format!("Der Ordner {dir} auf SSH · recorded-host konnte nicht aufgelistet werden.")
    );
    let unknown = "Could not list another opaque external diagnostic";
    assert_eq!(display_list_error(unknown, dir, &host), unknown);
    assert_eq!(
        display_list_error(&raw, "different original request", &host),
        raw
    );
    crate::i18n::set_language("en-US");
    assert_eq!(display_list_error(&raw, dir, &host), raw);
    assert_eq!(display_list_error(&malformed, dir, &host), malformed);
}
