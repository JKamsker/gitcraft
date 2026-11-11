use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn supplied() -> ReleaseView {
    ReleaseView {
        commit: "abcdef1234567890".into(),
        draft: Some(Ok(Draft {
            commit: "abcdef1234567890".into(),
            last_tag: None,
            changes: vec![],
            suggested: release::version::Version {
                prefix: "v".into(),
                major: 1,
                minor: 2,
                patch: 3,
                pre: String::new(),
            },
            branch: Some("feature/ß".into()),
            manifests: vec![],
            manifest_note: Some(
                "Only the tip of the branch checked out can move the version in its files".into(),
            ),
            changelog: false,
            sign: false,
            remote: Some("origin".into()),
            remote_ref: Some("refs/heads/feature/ß".into()),
            tags: vec!["v1.2.3".into()],
        })),
        form: Form {
            tag: "v1.2.3".into(),
            ..Form::default()
        },
        run: None,
    }
}

#[test]
fn fluent_partition_release_actual_paint_keeps_validation_notes_errors_and_plan_raw()
-> Result<(), String> {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let original = supplied();
        let expected_draft = original.draft.clone();
        let mut initialized = false;
        let mut h = Harness::new_ui_state(
            |ui, state: &mut ReleaseView| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    ui.ctx().request_discard("install supplied release fonts");
                    return;
                }
                contents(ui, state, Some(Forge::GitHub), 1000.0, &mut Vec::new());
            },
            original,
        );
        h.get_by_label(if language == "de" {
            "v1.2.3 ist bereits ein Tag"
        } else {
            "v1.2.3 is a tag already"
        });
        h.get_by_label(if language == "de" { "Nur an der Spitze des ausgecheckten Branches kann die Version in den Dateien geändert werden" } else { "Only the tip of the branch checked out can move the version in its files" });
        assert_eq!(h.state().draft, expected_draft);
        assert_eq!(h.state().form.tag, "v1.2.3");
        let draft = h
            .state()
            .draft
            .as_ref()
            .and_then(|x| x.as_ref().ok())
            .ok_or("missing supplied draft")?;
        assert_eq!(
            release::tag_problem(&h.state().form.tag, &draft.tags).as_deref(),
            Some("v1.2.3 is a tag already")
        );
        let plan = plan_of(draft, &h.state().form);
        let expected_preview = plan.preview(Some(Forge::GitHub));
        let original_failure = "Couldn't write Cargo.toml: Provider detail / ß";
        h.state_mut().run = Some(Run {
            plan,
            steps: vec![(release::Step::Tag, Status::Failed(original_failure.into()))],
            done: release::Done::default(),
            recorded: false,
            hosting: None,
            forge_repository_generation: 0,
        });
        h.run();
        h.get_by_label(if language == "de" {
            "Cargo.toml konnte nicht geschrieben werden: Provider detail / ß"
        } else {
            original_failure
        });
        let run = h.state().run.as_ref().ok_or("missing supplied run")?;
        assert_eq!(run.steps[0].1, Status::Failed(original_failure.into()));
        assert_eq!(run.steps[0].0.label(), "Tag");
        assert_eq!(run.plan.preview(Some(Forge::GitHub)), expected_preview);
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
