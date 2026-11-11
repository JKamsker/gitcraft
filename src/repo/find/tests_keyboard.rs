/// With the find bar open, a key typed into another field (the terminal, the commit message)
/// stays that field's: Ctrl+F and F3 are readline's and the pager's there.
#[test]
fn keys_typed_into_another_field_are_left_to_it() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/find-keys-test".into(), &ctx);
    tab.diff = Some(crate::repo::DiffView::new(DiffKey::Commit {
        hash: "abc".into(),
        path: "a.txt".into(),
    }));
    tab.find.show();
    let mut other = String::new();
    let mut frame = |tab: &mut RepoTab, events: Vec<Event>, focus: bool| {
        let mut left = Vec::new();
        let input = RawInput {
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            // As the tab does: its keys first, then what it draws.
            let free = !ui.ctx().egui_wants_keyboard_input();
            tab.find_keys(ui.ctx(), free);
            left = ui.input(|i| i.events.clone());
            let id = egui::Id::new("other");
            let _ = ui.add(egui::TextEdit::singleline(&mut other).id(id));
            if focus {
                ui.memory_mut(|m| m.request_focus(id));
            }
            // Like the terminal, it keeps Escape (for the shell).
            let keep = egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            };
            ui.memory_mut(|m| m.set_focus_lock_filter(id, keep));
        })
        .drop_without_applying_deltas();
        left
    };
    let key = |key, modifiers| {
        [true, false].map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
    };
    let kept = |left: &[Event], k: Key| {
        left.iter()
            .any(|e| matches!(e, Event::Key { key, pressed: true, .. } if *key == k))
    };
    let _ = frame(&mut tab, Vec::new(), true);
    tab.find.focus = false;
    let ctrl_f = frame(&mut tab, key(Key::F, Modifiers::COMMAND).to_vec(), false);
    let f3 = frame(&mut tab, key(Key::F3, Modifiers::NONE).to_vec(), false);
    let esc = frame(&mut tab, key(Key::Escape, Modifiers::NONE).to_vec(), false);
    assert!(kept(&ctrl_f, Key::F), "Ctrl+F");
    assert!(kept(&f3, Key::F3), "F3");
    assert!(kept(&esc, Key::Escape), "Esc");
    assert!(tab.find.open && !tab.find.focus);
    // With nothing focused, Ctrl+F is the find bar's.
    ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("other")));
    let ctrl_f = frame(&mut tab, key(Key::F, Modifiers::COMMAND).to_vec(), false);
    assert!(!kept(&ctrl_f, Key::F) && tab.find.focus);
}

#[test]
fn actual_markdown_diff_opens_find_and_marks_rendered_content() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/find-rendered-test".into(), &ctx);
    let mut view = crate::repo::DiffView::new(DiffKey::Commit {
        hash: "abc".into(),
        path: "README.md".into(),
    });
    view.rendered = Some(Ok(crate::repo::diff::rendered::RenderedMd {
        file: "# **Visible** words [label](https://hidden.example/secret)".into(),
        blocks: vec![(
            crate::repo::diff::rendered::Change::Added,
            "# **Visible** words [label](https://hidden.example/secret)".into(),
        )],
        before: false,
        images: Vec::new(),
    }));
    tab.diff = Some(view);
    tab.diff_rendered = true;
    assert!(tab.find_in().is_some());
    tab.find.show();
    tab.find.query.text = "Visible words".into();
    for _ in 0..2 {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            tab.diff_view(ui, &mut Vec::new())
        });
        output.drop_without_applying_deltas();
    }
    assert_eq!(tab.find.count(), 1);
    tab.find.query.text = "hidden.example".into();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.diff_view(ui, &mut Vec::new())
    });
    output.drop_without_applying_deltas();
    assert_eq!(tab.find.count(), 0);
    let request = tab.diff.as_ref().map(|v| v.request);
    if let Some(view) = &mut tab.diff {
        view.rendered_load += 1;
        view.rendered = Some(Ok(crate::repo::diff::rendered::RenderedMd {
            file: "# Replacement visible words".into(),
            blocks: vec![(
                crate::repo::diff::rendered::Change::Added,
                "# Replacement visible words".into(),
            )],
            before: false,
            images: Vec::new(),
        }));
    }
    tab.find.query.text = "Replacement".into();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.diff_view(ui, &mut Vec::new())
    });
    output.drop_without_applying_deltas();
    assert_eq!(request, tab.diff.as_ref().map(|v| v.request));
    assert_eq!(
        tab.find.count(),
        1,
        "accepted Markdown reread must replace the cached text under the same diff request"
    );
}
