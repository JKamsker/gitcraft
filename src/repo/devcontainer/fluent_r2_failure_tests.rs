use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn failure_receipt_repaints_full_original_guidance_and_preserves_supplied_state() {
    let detail = "opaque Docker failure\nWorking…\nKein Matrixeintrag ausgewählt\n{ $error }\nprovider: Start/rebuild may have partly completed.";
    let english = "Start/rebuild may have partly completed. Check Docker availability, installed Dev Container CLI, build files and lifecycle-command errors above. No global cleanup is performed; inspect this worktree's labels before retrying.";
    let german = "Start oder Neubau wurden möglicherweise teilweise ausgeführt. Prüfen Sie Docker, die installierte Dev Container CLI, Build-Dateien und die oben angezeigten Fehler der Lebenszyklusbefehle. Es erfolgt keine globale Bereinigung. Prüfen Sie vor einem neuen Versuch die Labels dieses Worktrees.";
    for created_in in ["en-US", "de"] {
        crate::i18n::set_language(created_in);
        let config = model::Config {
            path: "/supplied/worktree".into(),
            source: "opaque configuration".into(),
            context: Default::default(),
            linked: None,
        };
        let binding = model::Binding {
            config: config.clone(),
            engine: model::Engine {
                context: "owned-context".into(),
                id: "owned-engine".into(),
            },
            id: "a".repeat(64),
            folder: "/supplied/mount".into(),
            user: "opaque-user".into(),
        };
        let argv = model::arguments(&config, "up");
        let terminal = model::terminal(&binding);
        let mut state = State {
            open: true,
            checked: true,
            trust: true,
            rebuild: true,
            config: Some(config.clone()),
            binding: Some(binding),
            ..Default::default()
        };
        // The same receipt constructor is used by the actual Err(reply) arm.
        state.remember_result(LocalResult::Failed {
            detail: detail.into(),
        });
        let raw = format!("{detail}\n{english}");
        assert_eq!(state.result, raw);
        let mut h = Harness::new_ui_state(|ui, state: &mut State| state.result_ui(ui), state);
        for (lang, guidance) in [("en-US", english), ("de", german), ("en-US", english)] {
            crate::i18n::set_language(lang);
            h.run();
            let expected = format!("{detail}\n{guidance}");
            h.get_by_label(&expected);
            let state = h.state();
            assert_eq!(state.display_result(), expected);
            assert_eq!(state.result, raw);
            assert!(state.open && state.checked && state.trust && state.rebuild);
            assert!(state.pending.is_none());
            assert_eq!(state.config.as_ref(), Some(&config));
            assert_eq!(
                state.config.as_ref().map(|c| model::arguments(c, "up")),
                Some(argv.clone())
            );
            assert_eq!(
                state.binding.as_ref().map(model::terminal),
                Some(terminal.clone())
            );
            assert_eq!(
                state.binding.as_ref().map(|b| (
                    &b.engine.context,
                    &b.engine.id,
                    &b.id,
                    &b.folder,
                    &b.user
                )),
                Some((
                    &"owned-context".to_owned(),
                    &"owned-engine".to_owned(),
                    &"a".repeat(64),
                    &"/supplied/mount".to_owned(),
                    &"opaque-user".to_owned()
                ))
            );
        }
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn failure_receipt_requires_raw_agreement_and_does_not_parse_provider_detail() {
    let mut state = State::default();
    state.remember_result(LocalResult::Failed {
        detail: "opaque Docker failure".into(),
    });
    let envelope = state.result.clone();
    state.result =
        "provider replacement\nStart/rebuild may have partly completed.\nWorking…".into();
    let replacement = state.result.clone();
    for lang in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(lang);
        assert_eq!(state.display_result(), replacement);
        assert_eq!(state.result, replacement);
    }
    state.result = envelope.clone();
    state.result_presentation = None;
    for lang in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(lang);
        assert_eq!(state.display_result(), envelope);
        assert_eq!(state.result, envelope);
    }
    crate::i18n::set_language("en-US");
}
