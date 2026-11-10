use super::*;
use egui::Key;
use egui_kittest::{Harness, kittest::Queryable};

#[derive(Default)]
struct EnterForm {
    parent: String,
    parent_id: Option<egui::Id>,
    name: String,
    name_id: Option<egui::Id>,
    browse: usize,
    submit: usize,
    name_lost_on_enter: bool,
    browse_focused_on_enter: bool,
}

fn enter_form() -> Harness<'static, EnterForm> {
    let h = Harness::new_ui_state(
        |ui, form: &mut EnterForm| {
            let parent = ui.text_edit_singleline(&mut form.parent);
            form.parent_id = Some(parent.id);
            if singleline_enter(&parent) {
                form.submit += 1;
            }
            // Record intents only: canceling this simulated picker has no side effects.
            let browse = ui.button("Browse");
            if browse.clicked() {
                form.browse += 1;
            }
            let name = ui.text_edit_singleline(&mut form.name);
            form.name_id = Some(name.id);
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                form.name_lost_on_enter = name.lost_focus();
                form.browse_focused_on_enter = browse.has_focus();
            }
            if singleline_enter(&name) {
                form.submit += 1;
            }
        },
        EnterForm {
            parent: "/inert".into(),
            name: "repo".into(),
            ..Default::default()
        },
    );
    // Keep the focus transition and its observations in one rendering pass.
    h.ctx
        .options_mut(|o| o.max_passes = std::num::NonZeroUsize::MIN);
    h
}

#[test]
fn coalesced_tab_and_enter_does_not_submit() {
    for (empty, focus_name) in [(true, false), (false, false), (false, true)] {
        let mut h = enter_form();
        if empty {
            h.state_mut().parent.clear();
            h.state_mut().name.clear();
            h.step();
        }
        let field = if focus_name {
            h.state().name_id
        } else {
            h.state().parent_id
        }
        .expect("field rendered");
        h.ctx.memory_mut(|m| m.request_focus(field));
        h.step();
        // RawInput keeps both keys in one frame; Harness::key_press would split them.
        h.input_mut()
            .events
            .extend(
                [Key::Tab, Key::Enter]
                    .into_iter()
                    .map(|key| egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }),
            );
        h.step();
        // TextEdit handles Enter before registering Tab traversal and retains focus.
        // It must still not treat a traversal frame as a submission request.
        assert_eq!(h.ctx.memory(|m| m.focused()), Some(field));
        assert_eq!(h.state().browse, 0);
        assert_eq!(h.state().submit, 0);
    }
}

#[test]
fn enter_during_backward_focus_transition_only_activates_browse() {
    let mut h = enter_form();
    let name = h.state().name_id.expect("name rendered");
    h.ctx.memory_mut(|m| m.request_focus(name));
    h.step();
    h.key_down_modifiers(egui::Modifiers::SHIFT, Key::Tab);
    h.step();
    // Backward navigation takes effect in the next frame, alongside this Enter.
    h.key_down(Key::Enter);
    h.step();
    assert!(h.state().name_lost_on_enter);
    assert!(h.state().browse_focused_on_enter);
    assert_eq!(h.state().browse, 1);
    assert_eq!(h.state().submit, 0);
    h.key_up(Key::Enter);
    h.key_up(Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().submit, 0, "picker cancellation must not submit");
}

#[test]
fn enter_in_singleline_field_submits_after_surrendering_focus() {
    let mut h = enter_form();
    let name = h.state().name_id.expect("name rendered");
    h.ctx.memory_mut(|m| m.request_focus(name));
    h.step();
    h.key_press(Key::Enter);
    h.step();
    assert_eq!(h.state().browse, 0);
    assert_eq!(h.state().submit, 1);
    assert!(h.ctx.memory(|m| m.focused().is_none()));
}

fn production_dialog() -> Harness<'static, Option<App>> {
    let mut h = Harness::new_ui_state(
        |ui, state: &mut Option<App>| {
            if state.is_none() {
                let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                let mut app =
                    App::launched_using(&cc, None, || Ok("git version 2.43.0 (recorded)".into()));
                app.start_clone();
                let form = app.clone_form.as_mut().expect("clone form");
                form.url = "https://example.invalid/o/r".into();
                form.parent = "/inert".into();
                form.name = "repo".into();
                form.focus = false;
                *state = Some(app);
                return;
            }
            state.as_mut().expect("app").clone_dialog(ui.ctx());
        },
        None,
    );
    h.ctx
        .options_mut(|o| o.max_passes = std::num::NonZeroUsize::MIN);
    h.run_steps(3);
    h
}

fn record_clone_intents(work: impl FnOnce()) -> CloneIntents {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CLONE_INTENTS.with(|intents| *intents.borrow_mut() = None);
        }
    }
    CLONE_INTENTS.with(|intents| {
        assert!(intents.borrow().is_none());
        *intents.borrow_mut() = Some(Default::default());
    });
    let _reset = Reset;
    work();
    CLONE_INTENTS.with(|intents| intents.borrow_mut().take().expect("recording"))
}

#[test]
fn actual_clone_dialog_traversal_and_picker_cancellation_never_submit() {
    crate::i18n::set_language("en-US");
    let intents = record_clone_intents(|| {
        let mut h = production_dialog();
        h.get_all_by_role(egui::accesskit::Role::TextInput)
            .nth(2)
            .expect("name")
            .focus();
        h.step();
        h.key_down_modifiers(egui::Modifiers::SHIFT, Key::Tab);
        h.step();
        h.key_down(Key::Enter);
        h.step();
        h.key_up(Key::Enter);
        h.key_up(Key::Tab);
        h.run_steps(3);
        assert!(
            h.state()
                .as_ref()
                .is_some_and(|app| app.cloning.is_none() && app.clone_form.is_some())
        );
    });
    assert_eq!(intents.0, 1, "production Browse intent only");
    assert!(intents.1.is_empty());
    for field in [1, 2] {
        let intents = record_clone_intents(|| {
            let mut h = production_dialog();
            h.get_all_by_role(egui::accesskit::Role::TextInput)
                .nth(field)
                .expect("field")
                .focus();
            h.step();
            h.input_mut()
                .events
                .extend(
                    [Key::Tab, Key::Enter]
                        .into_iter()
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }),
                );
            h.step();
        });
        assert_eq!(intents, (0, Vec::new()));
    }
}

#[test]
fn actual_clone_dialog_singleline_enter_records_one_submission() {
    crate::i18n::set_language("en-US");
    let intents = record_clone_intents(|| {
        let mut h = production_dialog();
        h.get_all_by_role(egui::accesskit::Role::TextInput)
            .nth(2)
            .expect("name")
            .focus();
        h.step();
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(h.state().as_ref().is_some_and(|app| app.cloning.is_none()));
    });
    assert_eq!(
        intents,
        (
            0,
            vec![(
                "https://example.invalid/o/r".into(),
                "/inert".into(),
                "repo".into()
            )]
        )
    );
}

#[test]
fn enter_on_clone_cancel_closes_without_submitting() {
    crate::i18n::set_language("en-US");
    let mut h = Harness::new_ui_state(
        |ui, state: &mut Option<App>| {
            if state.is_none() {
                let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                let mut app =
                    App::launched_using(&cc, None, || Ok("git version 2.43.0 (recorded)".into()));
                app.start_clone();
                if let Some(form) = &mut app.clone_form {
                    form.url = "https://example.invalid/o/r".into();
                    form.parent = "/inert".into();
                    form.name = "repo".into();
                    form.depth = "invalid".into();
                    form.focus = false;
                }
                *state = Some(app);
                return;
            }
            if let Some(app) = state {
                app.clone_dialog(ui.ctx());
            }
        },
        None,
    );
    h.run_steps(3);
    h.get_by_label("Cancel").focus();
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(
        h.state()
            .as_ref()
            .is_some_and(|app| app.clone_form.is_none() && app.cloning.is_none())
    );
}

#[test]
fn actual_clone_fields_translate_options_without_rewriting_url_path_or_credentials() {
    crate::i18n::set_language("de");
    let form = CloneForm {
        options: Default::default(),
        depth: "0010".into(),
        sparse_folders: "src/日本語\ndocs".into(),
        url: "https://example.invalid/user/repo.git".into(),
        parent: "/opaque/日本語".into(),
        name: "repo".into(),
        name_edited: true,
        error: None,
        focus: false,
        auth: true,
        username: "literal-user".into(),
        password: "inert-secret".into(),
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, form: &mut CloneForm| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            form_fields(ui, form, &Tokens::get(ui.ctx()));
        },
        form,
    );
    h.run();
    h.get_by_label("Im Ordner");
    h.get_by_label("Klonoptionen").click();
    h.run();
    h.get_by_label("Ohne Blobs: Dateiinhalte bei Bedarf herunterladen")
        .click();
    h.run();
    assert!(h.state().options.blobless);
    h.get_by_label("Ausgewählte Ordner auschecken (Cone-Modus)")
        .click();
    h.run();
    assert_eq!(h.state().options.folders, Some(vec![]));
    h.get_by_label("Ordner relativ zum Repository, einer pro Zeile. Dateien im Stammverzeichnis sind immer enthalten.");
    assert_eq!(h.state().url, "https://example.invalid/user/repo.git");
    assert_eq!(h.state().parent, "/opaque/日本語");
    assert_eq!(h.state().depth, "0010");
    assert_eq!(h.state().sparse_folders, "src/日本語\ndocs");
    assert_eq!(h.state().username, "literal-user");
    assert_eq!(h.state().password, "inert-secret");
    crate::i18n::set_language("en-US");
    h.run();
    // The same expanded section retains its original persistent identity across locale changes.
    h.get_by_label("Depth (empty = full history)");
    h.get_by_label("Check out selected folders (cone mode)");
    assert!(h.state().options.blobless && h.state().options.folders.is_some());
}

#[test]
fn clone_messages_keep_raw_auth_and_cancellation_identities_and_opaque_details() {
    crate::i18n::set_language("de");
    for (raw, expected) in [
        (
            "Those credentials were refused.",
            "Diese Zugangsdaten wurden abgelehnt.",
        ),
        (
            "This repository needs you to sign in.",
            "Für dieses Repository musst du dich anmelden.",
        ),
        (
            "Depth must be a positive number, or empty for full history",
            "Die Tiefe muss eine positive Zahl sein oder für die vollständige Historie leer bleiben",
        ),
        (
            "Wait for the clone into /opaque/日本語 to finish, or cancel it on the start page",
            "Warte, bis das Klonen nach /opaque/日本語 abgeschlossen ist, oder brich es auf der Startseite ab",
        ),
        (
            "/opaque/日本語 already exists and isn't empty",
            "/opaque/日本語 existiert bereits und ist nicht leer",
        ),
        (
            "Clone into /opaque/日本語: cancelled",
            "Klonen nach /opaque/日本語: abgebrochen",
        ),
        (
            "Clone failed: External Git diagnostic 日本語",
            "Klonen fehlgeschlagen: External Git diagnostic 日本語",
        ),
    ] {
        assert_eq!(display_message(raw).as_deref(), Some(expected));
    }
    assert!(display_message("External Git diagnostic 日本語").is_none());
    assert_eq!(
        super::super::panels::display_app_message("External Git diagnostic 日本語"),
        "External Git diagnostic 日本語"
    );
    crate::i18n::set_language("en-US");
    assert_eq!(
        display_message("Clone into /opaque/日本語: cancelled").as_deref(),
        Some("Clone into /opaque/日本語: cancelled")
    );
}
