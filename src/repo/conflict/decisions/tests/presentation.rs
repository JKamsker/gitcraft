#[test]
fn hand_edits_keep_their_decision_when_they_can_be_told_apart() -> Result<(), String> {
    let dir = repo("manual")?;
    let mut ed = editor(&dir, "f.rs", FILE)?;
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.drafts = vec![Draft {
        note: "merged by hand".into(),
        ..Draft::default()
    }];
    ed.result = "1\n2\n3\nmerged by hand\n4\n5\n6\n".into();
    ed.manual = true;
    let kept = ed.to_record("me", 7)?;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].1.resolution, "merged by hand\n");
    assert_eq!(kept[0].1.at, 7);
    ed.result = "edited around\n".into();
    assert!(
        ed.to_record("me", 7)
            .is_err_and(|e| e.contains("edited by hand"))
    );
    // Without notes, nothing to keep.
    ed.memo.drafts = vec![Draft::default()];
    assert_eq!(ed.to_record("me", 7), Ok(Vec::new()));
    Ok(())
}

/// Drawn: the decision made before shows its reason and test, a stale one asks to be used,
/// and "Note why…" opens the fields.
#[test]
fn the_editor_shows_the_decision_and_its_note() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    type State = (RepoTab, Vec<crate::repo::Action>, bool);
    let dir = repo("drawn")?;
    let mut ed = editor(&dir, "f.rs", FILE)?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.spots = ed.decision_spots();
    ed.take_offers(vec![Some(Decision {
        note: "keep the old wire format".into(),
        test: "cargo test wire".into(),
        resolution: "old wire format\n".into(),
        ours: "new validation\n".into(),
        theirs: "old wire format\n".into(),
        before: "elsewhere\n".into(),
        by: "Zoë".into(),
        ..Decision::default()
    })]);
    ed.memo.drafts[0] = Draft::default();
    tab.conflict = Some(ed);
    let draw = |ui: &mut egui::Ui, (tab, acts, fonts): &mut State| {
        if !*fonts {
            crate::theme::install_fonts(ui.ctx());
            *fonts = true;
            return;
        }
        tab.conflict_view(ui, acts);
    };
    let mut harness = egui_kittest::Harness::new_ui_state(draw, (tab, Vec::new(), false));
    harness.run_steps(2);
    harness.get_by_label_contains("the code around it changed since");
    harness.get_by_label("“keep the old wire format”");
    harness.get_by_label_contains("Test “cargo test wire”: not run yet");
    harness.get_by_label("Use it anyway").click();
    // Background repository reads may request repaints independently of these clicks.
    // Advance the interaction without requiring the entire context to become idle.
    harness.run_steps(2);
    harness.get_by_label("Keep its note and test").click();
    harness.run_steps(2);
    harness.get_by_label("Done");
    let ed = harness.state().0.conflict.as_ref().ok_or("no editor")?;
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert!(ed.memo.drafts[0].open);
    assert_eq!(ed.memo.drafts[0].test, "cargo test wire");
    Ok(())
}
