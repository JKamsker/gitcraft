fn harness(r: &Repo, page: Page) -> Result<Harness<'static, (RepoTab, bool)>, String> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.work = crate::git::status(&r.0)?;
    tab.refs = crate::git::refs(&r.0)?;
    tab.open_file_comparison(page, &ctx);
    Ok(Harness::builder()
        .with_size(egui::vec2(1300., 950.))
        .build_ui_state(
            |ui, state: &mut (RepoTab, bool)| {
                if !state.1 {
                    crate::theme::install_fonts(ui.ctx());
                    state.1 = true;
                }
                state.0.poll(ui.ctx());
                let mut acts = Vec::new();
                state.0.file_comparison_view(ui, &mut acts);
                for action in acts {
                    state.0.apply(action, ui.ctx());
                }
            },
            (tab, false),
        ))
}
fn settle(h: &mut Harness<'_, (RepoTab, bool)>) -> Result<(), String> {
    // Windows metadata preservation starts a bounded native helper per snapshot.
    let seconds = if cfg!(windows) { 120 } else { 5 };
    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    loop {
        h.run_steps(1);
        if h.state().0.busy_op.is_none()
            && h.state()
                .0
                .file_comparison
                .as_ref()
                .is_none_or(|s| !s.loading)
        {
            break;
        }
        if std::time::Instant::now() > until {
            return Err("file view did not settle".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if let Some(state) = &h.state().0.file_comparison {
        if let Some(Err(e)) = &state.data {
            return Err(e.clone());
        }
        if let Some(Err(e)) = &state.pair {
            return Err(e.clone());
        }
    }
    Ok(())
}
#[test]
fn actual_lens_boundary_buttons_stage_and_unstage_the_displayed_snapshot() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "one\nbase\nlast\n")?;
    r.commit()?;
    r.write("a", "one\nstaged\nlast\n")?;
    r.git(&["add", "a"])?;
    r.write("a", "one\nbase\nlast\n")?;
    let mut h = harness(&r, Page::Lens)?;
    settle(&mut h)?;
    assert!(
        h.query_by_label_contains("Working copy matches HEAD")
            .is_some()
    );
    h.get_by_label("Unstage lens row 2").click();
    settle(&mut h)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "one\nbase\nlast");
    r.write("a", "one\nworking\nlast\n")?;
    h.get_by_label("Refresh file view").click();
    settle(&mut h)?;
    h.get_by_label("Stage lens row 2").click();
    settle(&mut h)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "one\nworking\nlast");
    assert_eq!(
        std::fs::read(r.0.join("a")).map_err(|e| e.to_string())?,
        b"one\nworking\nlast\n"
    );
    Ok(())
}
#[test]
fn actual_matrix_cells_open_a_read_only_content_diff_from_pinned_branches() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("config", "old\n")?;
    r.write("other", "other\n")?;
    r.commit()?;
    r.git(&["branch", "release-a"])?;
    r.write("config", "new\n")?;
    r.commit()?;
    r.git(&["branch", "release-b"])?;
    let mut h = harness(&r, Page::Matrix)?;
    settle(&mut h)?;
    {
        let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
        state.selected = ["refs/heads/release-a".into(), "refs/heads/release-b".into()]
            .into_iter()
            .collect();
    }
    h.get_by_label("Compare branch versions").click();
    settle(&mut h)?;
    h.query_by_label_contains("config | refs/heads/release-a |")
        .ok_or("first cell")?
        .click();
    h.run_steps(2);
    h.query_by_label_contains("config | refs/heads/release-b |")
        .ok_or("second cell")?
        .click();
    settle(&mut h)?;
    assert!(h.query_by_label_contains("Content edits only").is_some());
    assert!(h.query_by_label_contains("-old").is_some());
    assert!(h.query_by_label_contains("+new").is_some());
    assert!(h.query_by_label_contains("Stage lens row").is_none());
    let epoch = h
        .state()
        .0
        .file_comparison
        .as_ref()
        .ok_or("state")?
        .data_request;
    let old_request = h.state().0.file_comparison.as_ref().ok_or("state")?.request;
    h.query_by_label_contains("config | refs/heads/release-b |")
        .ok_or("deselect")?
        .click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Content edits only").is_none());
    {
        let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
        assert_eq!(state.cells.len(), 1);
        assert_eq!(state.data_request, epoch);
        assert!(!state.loading && state.pair.is_none());
        state.accept_pair(old_request, Err("late pair".into()));
        assert!(state.pair.is_none());
    }
    h.query_by_label_contains("config | refs/heads/release-a |")
        .ok_or("clear final")?
        .click();
    h.run_steps(2);
    assert!(
        h.state()
            .0
            .file_comparison
            .as_ref()
            .is_some_and(|s| s.cells.is_empty() && !s.loading && s.pair.is_none())
    );
    for label in [
        "config | refs/heads/release-a |",
        "config | refs/heads/release-b |",
    ] {
        h.query_by_label_contains(label).ok_or("pair cell")?.click();
        h.run_steps(2);
    }
    settle(&mut h)?;
    let old = h.state().0.file_comparison.as_ref().ok_or("state")?.request;
    h.query_by_label_contains("other | refs/heads/release-a |")
        .ok_or("new first")?
        .click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Content edits only").is_none());
    let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
    assert_eq!(state.cells.len(), 1);
    assert!(!state.loading && state.pair.is_none());
    state.accept_pair(old, Err("late prior selection".into()));
    assert!(state.pair.is_none());
    // Deliver a deterministically outstanding pair after clearing a selection.
    let stale = state.begin(false);
    let ctx = h.ctx.clone();
    h.state_mut()
        .0
        .comparison_cell(epoch, b"other".to_vec(), 0, &ctx);
    let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
    state.accept_pair(stale, Err("late pending pair".into()));
    assert!(state.pair.is_none() && !state.loading && state.cells.is_empty());
    // A queued old cell must not cancel a refresh of the matrix itself.
    let refresh = state.begin(true);
    h.state_mut()
        .0
        .comparison_cell(epoch, b"config".to_vec(), 0, &ctx);
    let state = h.state().0.file_comparison.as_ref().ok_or("state")?;
    assert_eq!(state.request, refresh);
    assert_eq!(state.data_request, 0);
    assert!(state.loading && state.data.is_none() && state.cells.is_empty());
    Ok(())
}
#[test]
fn actual_migration_ribbon_opens_content_edits_apart_from_the_rename() -> Result<(), String> {
    let r = Repo::new()?;
    r.write(
        "old/a",
        (0..30).map(|i| format!("line{i}\n")).collect::<String>(),
    )?;
    r.commit()?;
    r.git(&["mv", "old/a", "new-a"])?;
    r.write(
        "new-a",
        (0..30)
            .map(|i| format!("line{}\n", if i == 10 { 99 } else { i }))
            .collect::<String>(),
    )?;
    r.git(&["add", "-A"])?;
    let mut h = harness(&r, Page::Migration)?;
    settle(&mut h)?;
    h.query_by_label_contains("old/a → new-a · Rename")
        .ok_or("ribbon")?
        .click();
    settle(&mut h)?;
    assert!(h.query_by_label_contains("Content edits only").is_some());
    assert!(h.query_by_label_contains("+line99").is_some());
    assert!(h.query_by_label_contains("rename from").is_none());
    Ok(())
}
