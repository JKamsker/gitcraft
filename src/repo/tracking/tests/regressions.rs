use super::*;

fn loaded(form: &mut Form, view: View) {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(Ok(view));
    form.pending = Some(rx);
    let _ = form.settle();
}
fn harness(form: Form) -> Harness<'static, Form> {
    Harness::builder().build_ui_state(
        |ui, form: &mut Form| {
            let _ = form.show(ui, true);
        },
        form,
    )
}
#[test]
fn unresolvable_upstream_is_shown_unavailable_and_unset_is_enabled() -> Result<(), String> {
    let (repo, mut form) = fixture("tracking-ui-unresolvable")?;
    repo.git(&["config", "--unset-all", "remote.alice.fetch"])?;
    loaded(&mut form, tracking::read(repo.path(), "main")?);
    let mut h = harness(form);
    h.run();
    assert!(
        h.query_by_label_contains(crate::i18n::text("tracking-unresolved"))
            .is_some()
    );
    assert!(
        h.query_by_label(crate::i18n::text("tracking-unavailable"))
            .is_some()
    );
    h.get_by_label(crate::i18n::text("tracking-unset")).click();
    h.step();
    assert!(h.state().unset);
    Ok(())
}
#[test]
fn included_upstream_explains_why_both_edit_buttons_are_disabled() -> Result<(), String> {
    let (repo, mut form) = fixture("tracking-ui-inherited")?;
    let included = repo.path().join(".git/tracking-include");
    std::fs::write(&included, "[branch \"main\"]\nmerge = refs/heads/other\n")
        .map_err(|e| e.to_string())?;
    repo.git(&["config", "include.path", &included.to_string_lossy()])?;
    loaded(&mut form, tracking::read(repo.path(), "main")?);
    let why = form
        .view
        .as_ref()
        .and_then(|v| v.saving_blocked.clone())
        .ok_or("Expected inherited-config blocker")?;
    let mut h = harness(form);
    h.run();
    assert!(h.query_by_label(&why).is_some());
    for key in ["tracking-apply", "tracking-unset"] {
        h.get_by_label(crate::i18n::text(key)).click();
        h.step();
        assert!(h.state().pending.is_none());
        assert!(!h.state().applying);
    }
    Ok(())
}
#[test]
fn large_catalog_repaints_do_not_repeat_search_work() -> Result<(), String> {
    let (_repo, mut form) = fixture("tracking-ui-large")?;
    let view = form.view.as_mut().ok_or("Expected view")?;
    let seed = view.choices[0].clone();
    view.choices = (0..3000)
        .map(|index| {
            let mut choice = seed.clone();
            choice.remote = if index % 2 == 0 { "origin" } else { "alice" }.into();
            choice.path = format!("{}/töpíc-{index}", choice.remote);
            choice.full = format!("refs/remotes/{}", choice.path);
            choice
        })
        .collect();
    form.catalog = Catalog::new(view, &form.labels);
    let mut h = harness(form);
    h.run_steps(12);
    assert_eq!(h.state().catalog.examined, 3000);
    h.state_mut().query = "TÖPÍC-29".into();
    h.run_steps(12);
    assert_eq!(h.state().catalog.examined, 6000);
    assert!(!h.state().catalog.filtered.is_empty());
    h.state_mut().include_hidden = true;
    h.run_steps(12);
    assert_eq!(h.state().catalog.examined, 9000);
    h.state_mut().hidden.insert("alice".into());
    h.run_steps(12);
    assert_eq!(h.state().catalog.examined, 12000);
    // Snapshot replacement discards normalized text and filtered indices together.
    let form = h.state_mut();
    let view = form.view.take().ok_or("Expected view")?;
    loaded(form, view);
    h.run_steps(12);
    assert_eq!(h.state().catalog.examined, 3000);
    Ok(())
}
#[test]
fn remote_rename_while_open_rejects_apply_and_requires_reselection() -> Result<(), String> {
    let (repo, mut form) = fixture("tracking-ui-rename")?;
    repo.git(&["remote", "rename", "alice", "renamed"])?;
    form.applying = true;
    loaded(&mut form, tracking::read(repo.path(), "main")?);
    assert!(form.selected.is_none());
    assert_eq!(
        form.error.as_deref(),
        Some(crate::i18n::text("tracking-stale"))
    );
    Ok(())
}
#[test]
fn fetched_ref_can_be_selected_with_the_keyboard() -> Result<(), String> {
    let (_repo, mut form) = fixture("tracking-ui-keyboard")?;
    form.include_hidden = true;
    let mut h = harness(form);
    h.run();
    h.get_by_label_contains("origin/topic — origin/topic")
        .focus();
    h.key_press(egui::Key::Space);
    h.run_steps(3);
    assert_eq!(
        h.state().selected.as_deref(),
        Some("refs/remotes/origin/topic")
    );
    Ok(())
}

#[test]
fn overlapping_fetch_mappings_explain_why_a_fetched_ref_is_unavailable() -> Result<(), String> {
    let (repo, mut form) = fixture("tracking-ui-overlap")?;
    repo.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/alice/*",
    ])?;
    loaded(&mut form, tracking::read(repo.path(), "main")?);
    let mut h = harness(form);
    h.run();
    assert!(
        h.query_by_label(crate::i18n::text("tracking-ambiguous"))
            .is_some()
    );
    assert!(h.state().selected.is_none());
    assert!(!h.state().view.as_ref().is_some_and(|v| {
        v.choices
            .iter()
            .any(|c| c.full == "refs/remotes/alice/topic")
    }));
    Ok(())
}
