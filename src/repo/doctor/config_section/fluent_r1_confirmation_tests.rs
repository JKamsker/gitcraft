use super::*;
use egui_kittest::{Harness, kittest::Queryable};
fn check_cache(confirm: &Confirm, details: &[&str]) {
    let raw = (
        confirm.title.clone(),
        confirm.body.clone(),
        confirm.button.clone(),
        confirm.label.clone(),
        confirm.ops.clone(),
    );
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        let (title, body, button) = confirm.displayed();
        assert_eq!(title == raw.0, lang == "en-US");
        assert_eq!(button == raw.2, lang == "en-US");
        for detail in details {
            assert!(body.contains(detail), "{body}");
        }
        assert_eq!(
            (
                confirm.title.clone(),
                confirm.body.clone(),
                confirm.button.clone(),
                confirm.label.clone(),
                confirm.ops.clone()
            ),
            raw
        );
    }
}
#[test]
fn fluent_partition_r1_actual_remove_confirmation_keeps_opaque_value_path_and_argv()
-> Result<(), String> {
    for scope in [Scope::Local, Scope::Global, Scope::System] {
        let origin = Origin {
            scope,
            file: Some("raw;\npath/{$x}".into()),
            key: "user.name".into(),
            value: Some("Opaque;\nprovider {$x}".into()),
        };
        let op = vec![
            "config".into(),
            "--file".into(),
            origin.file.clone().ok_or("missing test value")?,
            "--unset-all".into(),
            origin.key.clone(),
        ];
        crate::i18n::set_language("de");
        let Action::Doctor(DoctorAct::Confirm(confirm)) = remove_confirm(&origin, op.clone())
        else {
            return Err("remove must confirm".into());
        };
        assert_eq!(confirm.ops, vec![op]);
        check_cache(
            &confirm,
            &[
                origin.shown_value(),
                origin.file.as_deref().ok_or("missing test value")?,
                &origin.key,
            ],
        );
        let mut opaque = confirm.clone();
        opaque.presentation = None;
        crate::i18n::set_language("de");
        assert_eq!(
            opaque.displayed(),
            (
                opaque.title.clone(),
                opaque.body.clone(),
                opaque.button.clone()
            )
        );
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
#[test]
fn fluent_partition_r1_actual_set_click_caches_raw_confirmation_and_projects_scope_at_paint()
-> Result<(), String> {
    for scope in [Scope::Global, Scope::System] {
        crate::i18n::set_language("de");
        let state = ConfigState {
            scope: Some(scope),
            value: "Opaque; {$x}".into(),
            ..Default::default()
        };
        let setting = config::find("user.name").ok_or("missing test value")?;
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (ConfigState, Vec<Action>, bool)| {
                if !std::mem::replace(&mut state.2, true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                set_row(
                    ui,
                    &mut state.0,
                    setting,
                    &Env::new(1, false, false),
                    &mut state.1,
                );
            },
            (state, Vec::new(), false),
        );
        h.run();
        h.get_by_label(crate::i18n::text("repo-workflows-doctor-set-row-b6f6f3ad"))
            .click();
        h.run();
        let actions = std::mem::take(&mut h.state_mut().1);
        let mut iter = actions.into_iter();
        let Action::Doctor(DoctorAct::Confirm(confirm)) =
            iter.next().ok_or("missing test value")?
        else {
            return Err("global/system must confirm".into());
        };
        assert!(iter.next().is_none());
        assert!(confirm.ops[0].contains(&"Opaque; {$x}".into()));
        check_cache(&confirm, &["user.name", "Opaque; {$x}"]);
        assert_eq!(h.state().0.scope, Some(scope));
        assert_eq!(h.state().0.value, "Opaque; {$x}");
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
