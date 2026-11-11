#[test]
fn actual_cross_file_move_opens_both_places_and_navigates_after_the_patch_arrives()
-> Result<(), String> {
    let r = Repo::new()?;
    let content = "fn important_validation_function(value: i32) {\n    validate_input(value);\n    record_the_validation_result(value);\n}\n";
    r.write("source.rs", content)?;
    r.commit()?;
    r.write("source.rs", "")?;
    r.write("destination.rs", content)?;
    r.git(&["add", "."])?;
    let mut h = harness(&r, Page::CodeMoves)?;
    settle(&mut h)?;
    h.get_by_label("source.rs:1 → destination.rs:1").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("M1 · Moved to").is_some());
    h.get_by_label("Open destination destination.rs:1").click();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        h.run_steps(1);
        if h.state().0.diff.as_ref().is_some_and(|v| v.data.is_some()) {
            break;
        }
        if std::time::Instant::now() > until {
            return Err("Destination patch did not arrive".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.state().0.file_comparison.is_none());
    let view = h.state().0.diff.as_ref().ok_or("destination")?;
    assert_eq!(view.key.path(), "destination.rs");
    let data = view
        .data
        .as_ref()
        .ok_or("patch")?
        .as_ref()
        .map_err(Clone::clone)?;
    assert_eq!(
        data.lines[view.jump_line.ok_or("line target")?].new,
        Some(1)
    );
    Ok(())
}

#[test]
fn line_history_header_wrap_toggles_without_reloading_or_losing_logical_selection()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("long.txt", format!("{} old\n", "source prose ".repeat(60)))?;
    r.commit()?;
    r.write("long.txt", format!("{} new\n", "source prose ".repeat(60)))?;
    let hash = r.commit()?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    let key = DiffKey::Commit {
        hash,
        path: "long.txt".into(),
    };
    let data = crate::repo::diff::load_lines(
        &r.0,
        &key,
        Some(&[(1, 1)]),
        crate::git::DiffOptions::default(),
        true,
    )?;
    let mut view = crate::repo::DiffView::new(key);
    view.next_request();
    view.lines = Some(vec![(1, 1)]);
    let target = data
        .lines
        .iter()
        .position(|l| l.new == Some(1))
        .ok_or("logical line")?;
    view.chosen.insert(target);
    view.data = Some(Ok(data));
    tab.diff = Some(view);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 950.))
        .build_ui_state(
            |ui, tab: &mut RepoTab| {
                let fonts = egui::Id::new("line-history-test-fonts");
                if !ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(fonts))
                    .unwrap_or(false)
                {
                    crate::theme::install_fonts(ui.ctx());
                    ui.ctx().data_mut(|d| d.insert_temp(fonts, true));
                    return;
                }
                let mut acts = Vec::new();
                tab.diff_view(ui, &mut acts);
                for action in acts {
                    tab.apply(action, ui.ctx());
                }
            },
            tab,
        );
    h.run_steps(2);
    let before = crate::settings::Settings::get(&h.ctx).diff_wrap;
    h.get_by_label("Wrap").click();
    h.run_steps(2);
    assert_ne!(crate::settings::Settings::get(&h.ctx).diff_wrap, before);
    let view = h.state().diff.as_ref().ok_or("diff")?;
    assert_eq!(view.lines, Some(vec![(1, 1)]));
    assert_eq!(view.chosen, [target].into());
    assert!(view.data.as_ref().is_some_and(Result::is_ok));
    h.get_by_label("Wrap").click();
    h.run_steps(2);
    assert_eq!(crate::settings::Settings::get(&h.ctx).diff_wrap, before);
    Ok(())
}
