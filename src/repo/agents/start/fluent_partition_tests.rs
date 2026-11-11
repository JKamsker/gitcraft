use super::*;
use egui_kittest::{Harness, kittest::Queryable};

// Supplied state only: no RepoTab/App construction, discovery, terminal or Git run.
fn supplied_form() -> StartForm {
    StartForm {
        task: "Keep --literal-pathspecs and refs/heads/topic".into(),
        branch: "topic/Überprüfung".into(),
        branch_edited: true,
        folder: "/supplied/work tree".into(),
        folder_edited: true,
        base: "refs/heads/main".into(),
        agent: KNOWN.len(),
        custom: "codex --model exact-model".into(),
        give_prompt: true,
        hooks: false,
        focus: false,
        repo: "/supplied/repository".into(),
        branches: vec![],
        folder_for: Box::new(|_| "unexpected-folder-resolution".into()),
    }
}

#[test]
fn fluent_partition_actual_start_form_keeps_action_payload_in_both_languages() -> Result<(), String>
{
    for (language, title, task, start) in [
        ("en-US", "Start agent task", "Task", "Start"),
        ("de", "Agent-Aufgabe starten", "Aufgabe", "Starten"),
    ] {
        crate::i18n::set_language(language);
        let mut initialized = false;
        let h = Harness::new_ui_state(
            |ui, form: &mut StartForm| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    ui.ctx().request_discard("Install supplied test fonts");
                    return;
                }
                let _ = form.ui(ui, false);
            },
            supplied_form(),
        );
        h.get_by_label(title);
        h.get_by_label(task);
        h.get_by_label(start);
        assert_eq!(h.state().agent_command(), "codex --model exact-model");
        assert_eq!(h.state().branch, "topic/Überprüfung");
        let mut recording_initialized = false;
        let recorded = Harness::new_ui_state(
            |ui, state: &mut (StartForm, Option<AgentAct>)| {
                if !recording_initialized {
                    crate::theme::install_fonts(ui.ctx());
                    recording_initialized = true;
                    ui.ctx().request_discard("Install supplied test fonts");
                    return;
                }
                state.1 = state.0.ui(ui, true);
            },
            (supplied_form(), None),
        );
        let Some(AgentAct::Start(payload)) = &recorded.state().1 else {
            return Err("expected recorded Start action".into());
        };
        assert_eq!(payload.agent, "codex --model exact-model");
        assert_eq!(payload.branch, "topic/Überprüfung");
        assert_eq!(payload.folder, "/supplied/work tree");
        assert_eq!(payload.base, "refs/heads/main");
        assert_eq!(
            payload.prompt,
            "Keep --literal-pathspecs and refs/heads/topic"
        );
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn fluent_partition_actual_validation_preserves_named_repository_content() {
    let mut form = supplied_form();
    form.branches.push(form.branch.clone());
    for (language, expected) in [
        (
            "en-US",
            "A branch named topic/Überprüfung exists already: pick another name",
        ),
        (
            "de",
            "Ein Branch namens topic/Überprüfung existiert bereits. Wählen Sie einen anderen Namen.",
        ),
    ] {
        crate::i18n::set_language(language);
        assert_eq!(
            form.problem(&Ok(form.folder.clone()), &form.agent_command())
                .as_deref(),
            Some(expected)
        );
    }
    crate::i18n::set_language("en-US");
}
