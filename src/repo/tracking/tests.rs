use super::*;
use crate::git::fixture::Repo;
use egui_kittest::{Harness, kittest::Queryable};
fn fixture(name: &str) -> Result<(Repo, Form), String> {
    let repo = Repo::new(name)?;
    let tip = repo.commit("base", &[])?;
    for alias in ["origin", "alice"] {
        repo.git(&[
            "remote",
            "add",
            alias,
            &format!("https://example.invalid/{alias}/repo"),
        ])?;
        repo.git(&["update-ref", &format!("refs/remotes/{alias}/topic"), &tip])?;
    }
    repo.git(&[
        "branch",
        "--set-upstream-to=refs/remotes/alice/topic",
        "main",
    ])?;
    let view = tracking::read(repo.path(), "main")?;
    let form = Form {
        repo: repo.path().to_owned(),
        branch: "main".into(),
        labels: Arc::default(),
        hidden: ["origin".into()].into_iter().collect(),
        pending: None,
        view: Some(view),
        selected: Some("refs/remotes/alice/topic".into()),
        query: String::new(),
        include_hidden: false,
        applying: false,
        unset: false,
        error: None,
    };
    Ok((repo, form))
}
#[test]
fn picker_groups_searches_and_includes_hidden_fetched_refs_without_commands() -> Result<(), String>
{
    let (_repo, form) = fixture("tracking-picker-ui")?;
    let mut h = Harness::builder().build_ui_state(
        |ui, form: &mut Form| {
            let _ = form.show(ui, true);
        },
        form,
    );
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        h.run();
        assert!(
            h.query_by_label_contains("alice/topic — alice/topic")
                .is_some()
        );
        assert!(
            h.query_by_label_contains("origin/topic — origin/topic")
                .is_none()
        );
        h.get_by_label(crate::i18n::text("tracking-hidden")).click();
        h.run();
        assert!(
            h.query_by_label_contains("origin/topic — origin/topic")
                .is_some()
        );
        h.state_mut().query = "origin".into();
        h.run();
        assert!(
            h.query_by_label_contains("alice/topic — alice/topic")
                .is_none()
        );
        assert!(
            h.query_by_label_contains("origin/topic — origin/topic")
                .is_some()
        );
    });
    assert!(jobs.is_empty());
    assert_eq!(
        h.state().selected.as_deref(),
        Some("refs/remotes/alice/topic")
    );
    Ok(())
}
#[test]
fn stale_dialog_refreshes_and_clears_selection_and_apply_uses_operation_pipeline()
-> Result<(), String> {
    let (repo, mut form) = fixture("tracking-picker-stale")?;
    repo.git(&["config", "branch.main.remote", "origin"])?;
    let (tx, rx) = mpsc::channel();
    tx.send(tracking::read(repo.path(), "main"))
        .map_err(|e| e.to_string())?;
    form.pending = Some(rx);
    form.applying = true;
    assert!(form.settle().is_none());
    assert!(form.selected.is_none());
    assert!(form.error.is_some());
    form.selected = Some("refs/remotes/alice/topic".into());
    let (tx, rx) = mpsc::channel();
    tx.send(tracking::read(repo.path(), "main"))
        .map_err(|e| e.to_string())?;
    form.pending = Some(rx);
    form.applying = true;
    let Some(Outcome::Done(action)) = form.settle() else {
        return Err("Expected ready operation".into());
    };
    let Action::UndoableOps { ops, undo, .. } = *action else {
        return Err("Expected undoable tracking operation".into());
    };
    let (result, _) = crate::repo::operations::run_steps(
        repo.path(),
        &ops,
        crate::repo::OpKind::Normal(undo, "tracking".into()),
        None,
    );
    result?;
    assert_eq!(repo.git(&["config", "branch.main.remote"])?.trim(), "alice");
    Ok(())
}

#[test]
fn every_local_menu_exposes_tracking_directly_including_head() {
    for (head, upstream, label) in [
        (true, "", "tracking-set"),
        (false, "alice/topic", "tracking-change"),
    ] {
        let ctx = egui::Context::default();
        let tab = RepoTab::new_deferred("/recording/tracking-menu".into(), &ctx);
        let branch = crate::git::Branch {
            name: "topic".into(),
            target: "a".repeat(40),
            is_head: head,
            upstream: upstream.into(),
            track: String::new(),
        };
        let mut h = Harness::builder().build_ui_state(
            move |ui, tab: &mut RepoTab| {
                tab.branch_menu_items(ui, &mut Vec::new(), &branch, true);
            },
            tab,
        );
        h.run();
        assert!(
            h.query_by_label(crate::i18n::text(label)).is_some(),
            "head={head}"
        );
    }
}

#[test]
fn dot_upstream_does_not_highlight_a_colliding_remote_path() -> Result<(), String> {
    let (repo, mut form) = fixture("tracking-picker-dot")?;
    repo.git(&["branch", "origin/topic"])?;
    repo.git(&[
        "branch",
        "--set-upstream-to=refs/heads/origin/topic",
        "main",
    ])?;
    let (tx, rx) = mpsc::channel();
    tx.send(tracking::read(repo.path(), "main"))
        .map_err(|e| e.to_string())?;
    form.pending = Some(rx);
    assert!(form.settle().is_none());
    assert!(form.selected.is_none());
    Ok(())
}
