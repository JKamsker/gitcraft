use super::*;
use crate::git::repository_tools::{guard::Snapshot, routes::PushRoute};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn form(preview: bool) -> Form {
    let mut form = Form::new(
        "/recorded/unopened-repo".into(),
        "töpíc".into(),
        vec!["alice".into(), "origin".into()],
    );
    if preview {
        form.preview = Some(PreparedPush {
            snapshot: Snapshot::recording(),
            route: PushRoute {
                branch: "töpíc".into(),
                commit: "a".repeat(40),
                remote: "alice".into(),
                destination: "refs/heads/release/töpíc".into(),
                urls: vec!["https://push.invalid/alice/repo".into()],
                lease: None,
            },
        });
    }
    form
}

#[test]
fn preview_confirm_only_hands_the_exact_selection_to_root_review() -> Result<(), String> {
    let mut h = Harness::builder().build_ui_state(
        |ui, state: &mut (Form, Vec<Outcome>)| {
            let outcome = state.0.show(ui, true);
            if !matches!(outcome, Outcome::Open) {
                state.1.push(outcome);
            }
        },
        (form(true), Vec::new()),
    );
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        h.run();
        for label in [
            "refs/heads/töpíc",
            "refs/heads/release/töpíc",
            "https://push.invalid/alice/repo",
        ] {
            assert!(h.query_by_label(label).is_some());
        }
        for _ in 0..8 {
            if h.get_by_label(crate::i18n::text("publish-confirm"))
                .accesskit_node()
                .is_focused()
            {
                break;
            }
            h.key_press(egui::Key::Tab);
            h.run();
        }
        assert!(
            h.get_by_label(crate::i18n::text("publish-confirm"))
                .accesskit_node()
                .is_focused()
        );
        h.key_press(egui::Key::Enter);
        h.run();
    });
    assert!(jobs.is_empty());
    let Some(Outcome::Done(action)) = h.state().1.first() else {
        return Err("Expected reviewed publication action".into());
    };
    let Action::RepositoryTools(Act::PublishPrepared(prepared)) = action.as_ref() else {
        return Err("Expected root push continuation".into());
    };
    assert_eq!(prepared.route.branch, "töpíc");
    assert_eq!(prepared.route.remote, "alice");
    assert_eq!(prepared.route.destination, "refs/heads/release/töpíc");
    assert!(prepared.route.lease.is_none());
    Ok(())
}

#[test]
fn known_default_warning_appears_in_both_preview_and_checklist_without_commands()
-> Result<(), String> {
    for known in [false, true] {
        let mut form = form(true);
        let prepared = form.preview.as_mut().ok_or("Expected preview")?;
        if known {
            prepared.snapshot.refs = format!(
                "refs/remotes/alice/HEAD\0{}\0refs/remotes/alice/release/töpíc\n",
                prepared.route.commit
            );
        }
        let mut plan =
            crate::push_preflight::selected_continuation_recording::plan(&prepared.route);
        plan.retain_selection(prepared.clone())?;
        let mut preview = Harness::builder().build_ui_state(
            |ui, form: &mut Form| {
                let _ = form.show(ui, true);
            },
            form,
        );
        let mut checklist = Harness::builder().build_ui_state(
            |ui, state: &mut (crate::push_preflight::Plan, bool)| {
                let _ = crate::repo::push_preflight::dialog(ui, &mut state.0, &mut state.1);
            },
            (plan, false),
        );
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            preview.run();
            checklist.run();
        });
        let warning = crate::i18n::text("publish-default-warning");
        assert_eq!(preview.query_by_label(warning).is_some(), known);
        assert_eq!(checklist.query_by_label(warning).is_some(), known);
        assert!(jobs.is_empty());
    }
    Ok(())
}

#[test]
fn cancelling_either_stage_is_inert_and_no_remote_explains_the_empty_state() {
    for preview in [false, true] {
        let mut h = Harness::builder().build_ui_state(
            |ui, state: &mut (Form, bool)| {
                state.1 |= matches!(state.0.show(ui, true), Outcome::Cancel);
            },
            (form(preview), false),
        );
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            h.run();
            h.get_by_label(crate::i18n::text("tracking-cancel")).click();
            h.run();
        });
        assert!(h.state().1);
        assert!(jobs.is_empty());
    }
    let mut h = Harness::builder().build_ui_state(
        |ui, form: &mut Form| {
            let _ = form.show(ui, true);
        },
        Form::new("/recorded/unopened-repo".into(), "main".into(), vec![]),
    );
    h.run();
    assert!(
        h.query_by_label(crate::i18n::text("publish-no-remotes"))
            .is_some()
    );
    assert!(
        h.get_by_label(crate::i18n::text("publish-preview"))
            .accesskit_node()
            .is_disabled()
    );
}
