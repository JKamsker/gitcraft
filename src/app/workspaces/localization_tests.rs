use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_workspace_row_keeps_raw_errors_and_keyboard_remove_under_german_labels()
-> Result<(), String> {
    let ui_errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let ui_errors_0 = ui_errors.clone();

    crate::i18n::set_language("de");
    let path = PathBuf::from("/workspace/日本語");
    let state = RepoState {
        seen: Some(Err("External Git diagnostic 日本語".into())),
        ..Default::default()
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, removed: &mut bool| {
            let ui_result = (|| -> Result<(), String> {
                if !fonts.replace(true) {
                    theme::install_fonts(ui.ctx());
                    return Ok(());
                }
                let t = Tokens::get(ui.ctx());
                *removed |= matches!(
                    workspace_row(ui, &path, Some(&state), &t),
                    Some(RowClick::Remove)
                );
                let tip = row_tooltip(&path, Some(&state));
                assert!(tip.starts_with("/workspace/日本語\n\n"));
                assert_eq!(
                    state
                        .seen
                        .as_ref()
                        .ok_or("missing test value")?
                        .as_ref()
                        .err()
                        .ok_or("expected an error")?,
                    "External Git diagnostic 日本語"
                );
                crate::a11y::end_frame(ui.ctx());

                Ok(())
            })();
            if let Err(error) = ui_result {
                ui_errors_0.borrow_mut().push(error);
            }
        },
        false,
    );
    h.run();
    h.get_by_label("/workspace/日本語 Konnte nicht geprüft werden: External Git diagnostic 日本語");
    h.key_press(egui::Key::Tab);
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(
        h.get_by_label("日本語 aus dem Arbeitsbereich entfernen")
            .is_focused()
    );
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(*h.state());
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("/workspace/日本語 Couldn't look at it: External Git diagnostic 日本語");

    if let Some(error) = ui_errors.borrow().first() {
        return Err(error.clone());
    }

    Ok(())
}

#[test]
fn locale_numbers_relative_age_and_error_classifiers_are_separate_from_raw_state()
-> Result<(), String> {
    crate::i18n::set_language("de");
    let overview = Overview {
        branch: Some("refs/feature-日本語".into()),
        changed: 1234,
        ahead: 2345,
        behind: 3456,
        has_remote: true,
        ..Default::default()
    };
    let t = Tokens::get(&egui::Context::default());
    let texts: Vec<_> = overview_parts(&overview, 0, &t)
        .into_iter()
        .map(|p| p.into_text())
        .collect();
    assert_eq!(
        texts,
        [
            "refs/feature-日本語",
            "1.234 geändert",
            "2.345",
            "3.456",
            "noch nie abgerufen"
        ]
    );
    assert_eq!(
        fetched_text(Some(100 - 7200), 100),
        "abgerufen vor 2 Stunden"
    );
    let raw = "fatal: not a git repository";
    assert_eq!(across::short_reason(raw), "not a repository");
    assert_eq!(across::display_reason(raw), "kein Repository");
    let mut workspaces = across::workspaces::Workspaces::default();
    let blank = workspaces.create(" ").err().ok_or("expected an error")?;
    assert_eq!(blank, "Give the workspace a name.");
    assert_eq!(
        across::workspaces::display_error(&blank),
        "Gib dem Arbeitsbereich einen Namen."
    );
    workspaces
        .create("Opaque 日本語")
        .map_err(|error| format!("{error:?}"))?;
    let duplicate = workspaces
        .create("Opaque 日本語")
        .err()
        .ok_or("expected an error")?;
    assert_eq!(
        duplicate,
        "There is a workspace called “Opaque 日本語” already."
    );
    assert_eq!(
        across::workspaces::display_error(&duplicate),
        "Ein Arbeitsbereich namens „Opaque 日本語“ ist bereits vorhanden."
    );
    assert_eq!(
        across::workspaces::display_error("External provider output 日本語"),
        "External provider output 日本語"
    );
    crate::i18n::set_language("en-US");
    assert_eq!(across::workspaces::display_error(&duplicate), duplicate);
    assert_eq!(across::display_reason(raw), "not a repository");

    Ok(())
}
