// Included in the parent test module to preserve qualified test names.

#[test]
fn pane_keys_file_navigation_stage_unstage_and_open_use_the_normal_git_workers()
-> Result<(), String> {
    let mut t = Test::new("file-actions")?;
    t.frame(vec![]);
    t.pane(Key::Num1);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Sidebar));
    let shapes = t.frame(vec![]);
    let accent = Tokens::get(&t.ctx).accent;
    assert!(
        shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r)
        if r.stroke.width == 2.0 && r.stroke.color == accent && r.rect.left() < 5.0
            && r.rect.width() > 170.0 && r.rect.width() < 520.0)),
        "sidebar focus must have a visible outline"
    );
    t.pane(Key::Num2);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Graph));
    t.pane(Key::Num3);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Detail));
    assert!(
        t.tab
            .keyboard_file_selected(&DiffKey::Work(t.tab.work.unstaged[0].clone()))
    );
    t.key(Key::J);
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(f))) if f.path == "b.txt")
    );
    t.key(Key::K);
    t.frame(vec![Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    t.wait()?;
    assert_eq!(t.git(&["diff", "--cached", "--name-only"])?.trim(), "a.txt");
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(f))) if f.path == "a.txt" && f.area == Area::Staged)
    );
    t.frame(keys(Key::Space, Modifiers::NONE, true));
    t.wait()?;
    assert_eq!(
        t.git(&["diff", "--cached", "--name-only"])?.trim(),
        "a.txt",
        "held Space must not unstage it again"
    );
    t.key(Key::Space);
    t.wait()?;
    assert!(
        t.git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
    );
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        matches!(&t.tab.diff, Some(view) if matches!(&view.key, DiffKey::Work(f) if f.path == "a.txt"))
    );
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Graph));
    t.pane(Key::Num3);
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        t.tab.diff.is_some(),
        "Enter focuses an already open diff rather than toggling it closed"
    );
    Ok(())
}

#[test]
fn current_hunk_keys_stage_unstage_and_confirm_discard_with_undo() -> Result<(), String> {
    let mut t = Test::new("hunk-actions")?;
    t.frame(vec![]);
    t.pane(Key::Num3);
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        t.tab
            .diff
            .as_ref()
            .is_some_and(|v| v.keyboard_hunk.is_some())
    );
    t.frame(vec![Event::Key {
        key: Key::S,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    t.wait()?;
    let staged = t.git(&["diff", "--cached"])?;
    assert!(staged.contains("first edit") && !staged.contains("second edit"));
    let unchanged = t.git(&["diff"])?;
    t.frame(keys(Key::S, Modifiers::NONE, true));
    t.wait()?;
    assert!(t.tab.dialog.is_none());
    assert_eq!(
        t.git(&["diff"])?,
        unchanged,
        "held S must not stage the next hunk"
    );
    t.key(Key::D);
    assert!(matches!(t.tab.dialog, Some(Dialog::KeyboardDiscard { .. })));
    assert_eq!(
        t.git(&["diff"])?,
        unchanged,
        "D must not discard before confirmation"
    );
    t.frame(vec![]); // Let the newly measured modal settle at its centered position.
    let shapes = t.frame(vec![]);
    let point = text_center(&shapes, "Discard hunk");
    assert!(point.is_some());
    if let Some(point) = point {
        t.frame(vec![Event::PointerMoved(point)]);
        t.frame(vec![Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        t.frame(vec![Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
    }
    t.wait()?;
    assert!(
        !t.git(&["diff"])?.contains("second edit"),
        "discard state: status={:?}, dialog={}, busy={:?}, work_read={}, pending={}, diff_request={:?}",
        t.tab.status,
        t.tab.dialog.is_some(),
        t.tab.busy_op,
        t.tab.work_read,
        t.tab.work_pending,
        t.tab.diff.as_ref().map(|v| v.request)
    );
    assert!(
        t.tab
            .timeline
            .current_entry()
            .is_some_and(|e| e.undo.is_some())
    );
    t.tab.apply(Action::Undo, &t.ctx);
    t.wait()?;
    assert!(t.git(&["diff"])?.contains("second edit"));
    t.tab.apply(Action::CloseDiff, &t.ctx);
    t.pane(Key::Num3);
    // Move from unstaged a to b, then to staged a, without staging b.
    t.key(Key::J);
    t.key(Key::J);
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        matches!(&t.tab.diff, Some(v) if matches!(&v.key, DiffKey::Work(f) if f.area == Area::Staged))
    );
    t.key(Key::U);
    t.wait()?;
    assert!(
        t.git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
    );
    Ok(())
}
