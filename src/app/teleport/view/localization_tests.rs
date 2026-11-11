use super::*;
use crate::teleport::Source;
use egui_kittest::{Harness, kittest::Queryable};
use std::path::PathBuf;

fn form() -> TeleportForm {
    TeleportForm {
        source: PathBuf::from("/opaque/source"),
        info: Loading::Ready(Source {
            repo: PathBuf::from("/opaque/source"),
            head: "12345678abcdef".into(),
            branch: None,
            upstream: None,
            remotes: vec![],
            object_format: "sha1".into(),
        }),
        distros: None,
        wsl: vec![],
        ssh_hosts: vec![],
        typed_host: String::new(),
        known: vec![],
        machine: Some(Machine::There(remote::Host::Ssh(
            "me@example.invalid".into(),
        ))),
        pick_after_read: None,
        home: Loading::Ready("/home/me".into()),
        clones: Loading::Ready(vec![PathBuf::from("ssh://me@example.invalid/srv/literal")]),
        place: Place::Found(0),
        place_picked: false,
        other_repo: "~/other 日本語".into(),
        new_repo: "~/new 日本語".into(),
        clone_url: "https://example.invalid/literal.git".into(),
        branch: "literal-branch".into(),
        folder: "~/worktree 日本語".into(),
        folder_edited: true,
        error: Some("Teleport failed: Some files have conflicts: resolve them first.".into()),
        running: None,
    }
}

#[test]
fn actual_supplied_teleport_fields_translate_while_recorded_plan_and_destination_values_stay_literal()
-> Result<(), String> {
    crate::i18n::set_language("de");
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, form: &mut TeleportForm| {
            if !fonts.replace(true) {
                theme::install_fonts(ui.ctx());
                return;
            }
            let t = Tokens::get(ui.ctx());
            assert!(!fields(ui, form, &t, true));
        },
        form(),
    );
    h.run();
    h.get_by_label("/opaque/source  ·  losgelöst bei 12345678");
    h.get_by_label(
        "Teleportieren fehlgeschlagen: Einige Dateien enthalten Konflikte: Löse diese zuerst.",
    );
    assert_eq!(
        h.state().error.as_deref(),
        Some("Teleport failed: Some files have conflicts: resolve them first.")
    );
    h.get_by_label("Neuer Worktree eines anderen Repositorys dort")
        .click();
    h.run();
    assert_eq!(h.state().place, Place::Other);
    assert_eq!(h.state().other_repo, "~/other 日本語");
    let plan = h.state().plan()?;
    let raw_destination = format!("{:?}", plan.destination);
    let raw_folder = plan.folder.clone();
    assert_eq!(plan.branch.as_deref(), Some("literal-branch"));
    assert_eq!(plan.source, PathBuf::from("/opaque/source"));
    assert_eq!(plan.confirm_above, crate::teleport::LARGE);
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("A new worktree of another repository there");
    let same = h.state().plan()?;
    assert_eq!(format!("{:?}", same.destination), raw_destination);
    assert_eq!(same.folder, raw_folder);
    assert!(h.state().running.is_none() && h.state().distros.is_none());
    Ok(())
}
