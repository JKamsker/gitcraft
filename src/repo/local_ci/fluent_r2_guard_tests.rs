use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn remotes() -> Vec<crate::git::Remote> {
    vec![crate::git::Remote {
        name: "fork".into(),
        url: "https://github.com/supplied/opaque-repo.git".into(),
    }]
}
fn supplied_state() -> Result<State, String> {
    let state = State {
        workflows: vec![model::parse("opaque.yml".into(), "on: [push]\njobs:\n  opaque-job:\n    runs-on: ubuntu-latest\n    steps: [{run: 'opaque command --unchanged'}]\n".into())?],
        remote: "fork".into(),
        remote_override: true,
        displayed_context: Some(context::Context::capture("fork", &remotes())?),
        payload: "{\"opaque\":\"Working…\"}".into(),
        platforms: "ubuntu-latest=supplied/image".into(),
        architecture: "linux/arm64".into(),
        minutes: 17,
        trust: true,
        provenance: Some("opaque prior provenance\nKein Matrixeintrag ausgewählt".into()),
        ..Default::default()
    };
    Ok(state)
}
fn snapshot(state: &State) -> Result<String, String> {
    let log = state.log.lock().map_err(|e| e.to_string())?;
    Ok(format!(
        "{:?}",
        (
            (
                &state.workflows[0].file,
                &state.workflows[0].source,
                &state.workflows[0].events,
                &state.workflows[0].jobs
            ),
            (state.workflow, state.event, state.job, state.matrix),
            (
                &state.remote,
                state.remote_override,
                &state.displayed_context
            ),
            (
                &state.payload,
                &state.platforms,
                &state.architecture,
                state.minutes
            ),
            (state.trust, state.busy(), &state.provenance),
            (Arc::as_ptr(&state.log), &log.text, &log.rows, log.truncated),
        )
    ))
}
fn repaint_same_state(state: State, raw: &str, german: &str) -> Result<(), String> {
    let before = snapshot(&state)?;
    let mut h = Harness::new_ui_state(
        |ui, state: &mut State| {
            let mut actions = Vec::new();
            state.controls(ui, &mut actions, &remotes());
            assert!(actions.is_empty());
        },
        state,
    );
    for (lang, expected) in [("en-US", raw), ("de", german), ("en-US", raw)] {
        crate::i18n::set_language(lang);
        h.run();
        h.get_by_label(expected);
        assert_eq!(h.state().display_result_error(raw), expected);
        assert_eq!(h.state().result.as_ref(), Some(&Err(raw.to_owned())));
        assert_eq!(snapshot(h.state())?, before);
        assert!(h.state().pending.is_none());
    }
    Ok(())
}

#[test]
fn actual_context_guard_retains_raw_error_and_repaints_without_restoring_consent()
-> Result<(), String> {
    for created_in in ["en-US", "de"] {
        crate::i18n::set_language(created_in);
        let mut state = supplied_state()?;
        let old = snapshot(&state)?;
        let displayed = state
            .displayed_context
            .clone()
            .ok_or("missing supplied context")?;
        assert!(state.run_context_matches(&displayed));
        assert_eq!(snapshot(&state)?, old);
        assert!(state.result.is_none());
        let changed = context::Context::capture(
            "fork",
            &[crate::git::Remote {
                name: "fork".into(),
                url: "https://github.com/replacement/repo.git".into(),
            }],
        )?;
        assert!(!state.run_context_matches(&changed));
        assert!(!state.trust);
        assert_eq!(state.displayed_context.as_ref(), Some(&displayed));
        repaint_same_state(
            state,
            "Repository context changed since display; review it before running",
            "Der Repository-Kontext hat sich seit der Anzeige geändert. Vor dem Start erneut prüfen.",
        )?;
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn actual_matrix_guard_retains_raw_error_and_selection_eligibility_without_submitting()
-> Result<(), String> {
    for created_in in ["en-US", "de"] {
        crate::i18n::set_language(created_in);
        let mut state = supplied_state()?;
        let entries = state.workflows[0].matrices("opaque-job")?;
        let before = snapshot(&state)?;
        assert_eq!(state.run_matrix(&entries), Some(entries[0].clone()));
        assert_eq!(snapshot(&state)?, before);
        assert!(state.result.is_none());
        state.matrix = entries.len();
        assert!(state.run_matrix(&entries).is_none());
        assert!(state.trust);
        assert!(entries.get(state.matrix).is_none());
        repaint_same_state(
            state,
            "No selected matrix entry",
            "Kein Matrixeintrag ausgewählt",
        )?;
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn local_guard_receipt_rejects_mismatch_and_leaves_unmarked_provider_details_opaque()
-> Result<(), String> {
    let mut state = supplied_state()?;
    state.remember_failure(ResultFailure::MissingMatrix);
    state.result = Some(Err(
        "provider diagnostic\nNo selected matrix entry\nKein Matrixeintrag ausgewählt".into(),
    ));
    for lang in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(lang);
        let raw = state
            .result
            .as_ref()
            .and_then(|r| r.as_ref().err())
            .ok_or("missing supplied failure")?;
        assert_eq!(state.display_result_error(raw), *raw);
    }
    state.result_presentation = None;
    for raw in [
        "No selected matrix entry",
        "Repository context changed since display; review it before running",
        "Kein Matrixeintrag ausgewählt",
    ] {
        state.result = Some(Err(raw.into()));
        for lang in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(lang);
            assert_eq!(state.display_result_error(raw), raw);
            assert_eq!(state.result.as_ref(), Some(&Err(raw.to_owned())));
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
