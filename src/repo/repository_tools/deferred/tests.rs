use super::*;
#[test]
fn pure_deferred_panel_keeps_conflicts_reachable_and_rejects_late_marker_snapshots()
-> Result<(), String> {
    let view = logic::tests::view();
    let mut cache = Cache::default();
    cache.observe(20, &view);
    assert_eq!(cache.markers.len(), 1);
    let mut closed = view.clone();
    closed.jobs.jobs[0].cancelled = true;
    cache.observe(21, &closed);
    cache.observe(20, &view);
    assert!(
        cache.markers.is_empty(),
        "late reads cannot restore a closed marker"
    );
    let path = view.jobs.jobs[0].steps[0].worktree.clone();
    let mut state = State {
        id: 100,
        query: String::new(),
        result: Some(Ok(Data::Deferred(view))),
        filter: String::new(),
        tool: super::super::Tool::Deferred,
        acknowledged: false,
        compatible: false,
        parent: String::new(),
        effective: true,
    };
    let ctx = egui::Context::default();
    let mut acts = Vec::new();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 1600.0),
            )),
            ..Default::default()
        },
        |ui| show(ui, &mut state, &mut acts),
    );
    let texts = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(texts.contains(&"Conflict waiting: src/shared.rs"));
    assert!(texts.contains(&"Continue / reconcile candidate worktree"));
    assert!(texts.contains(&"Reconcile exact acceptance receipts"));
    assert!(!texts.contains(&"Accept ready candidates"));
    let pos = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == path.to_string_lossy() => {
                Some(t.pos + t.galley.size() / 2.0)
            }
            _ => None,
        })
        .ok_or("retained candidate worktree link")?;
    output.textures_delta.clear();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        },
        |ui| show(ui, &mut state, &mut acts),
    );
    output.textures_delta.clear();
    assert!(
        acts.iter()
            .any(|a| matches!(a,Action::OpenWorktree(p) if p==&path))
    );
    Ok(())
}
