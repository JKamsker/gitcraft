/// Frames as the window draws them, with time for the background work to come back.
fn frames(harness: &mut egui_kittest::Harness<'_, RepoTab>) {
    for _ in 0..20 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Drawn as the window draws it: Alt+↓ moves in the graph alone; with the diff below the
/// graph, Alt+← goes back to the graph, Alt+→ forward to the diff again, Esc closes it.
#[test]
fn the_keys_step_through_the_drawn_tab() {
    let (dir, file) = repo("drawn");
    let ctx = egui::Context::default();
    let tab = opened(&dir, &ctx);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(
            |ui, tab: &mut RepoTab| {
                // The app's fonts are there from the frame after they are given.
                if ui.ctx().cumulative_frame_nr() == 0 {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                } else {
                    tab.ui(ui);
                    // Its widgets named, to be found by name.
                    crate::a11y::end_frame(ui.ctx());
                }
            },
            tab,
        );
    below_the_graph(&harness.ctx, true);
    frames(&mut harness);
    let (head, base) = head_and_base(&dir);
    assert_eq!(harness.state().selected_hash(), Some(head.clone()));
    // The graph uses its registered default; Alt+Down belongs to diff stepping.
    harness.key_press(Key::ArrowDown);
    frames(&mut harness);
    assert_eq!(harness.state().selected_hash(), Some(base));
    harness.key_press(Key::ArrowUp);
    frames(&mut harness);

    let key = commit_key(&head, &file);
    assert_eq!(harness.state().diff.as_ref().map(|view| &view.key), Some(&key));
    // History navigation already revealed it; start the explicit manual-open path
    // from the graph, retaining the original split and back/forward assertions.
    harness.state_mut().queue(Action::CloseDiff);
    frames(&mut harness);
    assert!(harness.state().diff.is_none());
    harness.state_mut().queue(Action::OpenDiff(key.clone()));
    frames(&mut harness);
    assert!(harness.state().diff_beside_graph(&harness.ctx));
    // Drawn, with its way back: not the commit-split editor's (empty) view in its place.
    // Its close button, named without the key in its tooltip ("Close (Esc)").
    let close = crate::i18n::text("repo-core-0711");
    let close = close.split(" (").next().unwrap_or(close);
    assert!(
        harness.query_by_label(close).is_some(),
        "the diff below the graph isn't drawn"
    );
    // Alt+↓ is the diff's now: the graph's selection stays.
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    frames(&mut harness);
    assert_eq!(harness.state().selected_hash(), Some(head));

    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    frames(&mut harness);
    assert!(harness.state().diff.is_none());
    harness.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
    frames(&mut harness);
    let shown = harness.state().diff.as_ref().map(|d| d.key.clone());
    assert_eq!(shown, Some(key));
    harness.key_press(Key::Escape);
    frames(&mut harness);
    assert!(harness.state().diff.is_none());
    // Esc went to the graph: back is the diff again.
    assert!(matches!(
        harness.state().nav.peek(-1),
        Some(View::Diff { .. })
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
