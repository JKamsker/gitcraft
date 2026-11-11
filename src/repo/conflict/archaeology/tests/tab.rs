fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Through the tab, as the conflict editor asks: the timelines are read, the pair found and a
/// commit's change shown; an editor opened again meanwhile takes none of it.
#[test]
fn the_tab_reads_and_searches_for_the_open_editor() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("tab", path)?;
    let _ = git(&dir, &["merge", "side"]);
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.work.in_progress = Some(InProgress::Merge);
    let file = StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    };
    tab.open_diff(DiffKey::Work(file), &ctx);
    fn history(tab: &RepoTab) -> Option<&History> {
        tab.conflict.as_ref().map(|ed| &ed.history)
    }
    tab.archaeology_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        history(tab).is_some_and(|h| matches!(h.timelines, Read::Done(_)))
    });
    tab.archaeology_act(Act::Find, &ctx);
    wait(&mut tab, &ctx, |tab| {
        history(tab)
            .and_then(|h| h.search.as_ref())
            .is_some_and(|s| s.result != Read::Reading)
    });
    let found = history(&tab)
        .and_then(|h| h.search.as_ref())
        .map(|s| s.result.clone());
    let hash = match history(&tab).map(|h| &h.timelines) {
        Some(Read::Done(t)) => t.ours.first().map(|s| s.hash.clone()),
        _ => None,
    }
    .ok_or("no commits")?;
    tab.archaeology_act(Act::ShowChange(hash.clone(), "short".to_owned()), &ctx);
    wait(&mut tab, &ctx, |tab| {
        history(tab)
            .and_then(|h| h.change.as_ref())
            .is_some_and(|(_, _, r)| *r != Read::Reading)
    });
    let shown = history(&tab).and_then(|h| h.change.clone());
    // Asked again for the same commit: hidden.
    tab.archaeology_act(Act::ShowChange(hash.clone(), "short".to_owned()), &ctx);
    let hidden = history(&tab).is_some_and(|h| h.change.is_none());

    // A read for an earlier opening isn't taken by the one open now: its answer is caught and
    // handed to the tab after the editor was opened again.
    let (tx, rx) = std::sync::mpsc::channel();
    tab.tx.sender = tx;
    if let Some(ed) = tab.conflict.as_mut() {
        ed.history.timelines = Read::NotAsked;
    }
    tab.archaeology_act(Act::Load, &ctx);
    let started = history(&tab).is_some_and(|h| h.timelines == Read::Reading);
    if let Some(ed) = tab.conflict.as_mut() {
        ed.generation += 1;
        ed.history = History::default();
    }
    let delivered = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(crate::repo::messages::Msg::Then(then)) => {
            then(&mut tab, &ctx);
            true
        }
        _ => false,
    };
    let untouched = history(&tab).is_some_and(|h| h.timelines == Read::NotAsked);
    assert!(started && delivered);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        found,
        Some(Read::Done(Found::Pair {
            ours: 2,
            theirs: 1,
            checked: true
        }))
    );
    match shown {
        Some((h, _, Read::Done(text))) => {
            assert_eq!(h, hash);
            assert!(text.contains("+main one"), "{text}");
        }
        other => return Err(format!("{other:?}")),
    }
    assert!(hidden);
    assert!(untouched);
    Ok(())
}

/// Drawn: the editor asks for the timelines as it shows, lists each side's commits with the
/// pair found lit and named, and a click on a commit asks for its change.
#[test]
fn the_strip_shows_both_sides_and_the_answer() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    type State = (RepoTab, Vec<crate::repo::Action>, bool);
    let dir = std::env::temp_dir().join(format!("gitcraft-arch-drawn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("f"), "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n")
        .map_err(|e| e.to_string())?;
    let ed = ConflictEditor::open(&dir, "f", Some(InProgress::Merge)).ok_or("not opened")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    tab.conflict = Some(ed);
    tab.work.in_progress = Some(InProgress::Merge);
    let draw = |ui: &mut egui::Ui, (tab, acts, fonts): &mut State| {
        // gitcraft's fonts first: they apply from the next frame.
        if !*fonts {
            crate::theme::install_fonts(ui.ctx());
            *fonts = true;
            return;
        }
        tab.conflict_view(ui, acts);
    };
    let mut harness = egui_kittest::Harness::new_ui_state(draw, (tab, Vec::new(), false));
    harness.run_steps(2);
    let asked = harness
        .state()
        .1
        .iter()
        .any(|a| matches!(a, crate::repo::Action::Archaeology(Act::Load)));
    let step = |short: &str, summary: &str| Step {
        hash: format!("{short}ffff"),
        short: short.to_owned(),
        author: "Zoë".to_owned(),
        time: 0,
        summary: summary.to_owned(),
    };
    if let Some(ed) = harness.state_mut().0.conflict.as_mut() {
        ed.history.timelines = Read::Done(Timelines {
            ours: vec![step("a1", "add caching"), step("a2", "tune")],
            theirs: vec![step("b1", "change ownership")],
            truncated: (false, false),
            base: None,
            theirs_rev: None,
        });
        ed.history.search = Some(Search {
            cancel: Default::default(),
            merges: Default::default(),
            result: Read::Done(Found::Pair {
                ours: 0,
                theirs: 0,
                checked: true,
            }),
        });
    }
    harness.run();
    harness.get_by_label("Find where it started");
    harness.get_by_label_contains("Where it started: a1 “add caching”");
    // Picking a commit on each side offers its change and a test merge of the two.
    harness.get_by_label_contains("a2  tune").click();
    harness.run();
    harness
        .get_by_label_contains("b1  change ownership")
        .click();
    harness.run();
    harness.get_by_label("What b1 changed").click();
    harness.get_by_label("Test these two").click();
    harness.run();
    let state = harness.state();
    let acts = &state.1;
    assert!(asked);
    assert!(acts.iter().any(|a| matches!(a,
        crate::repo::Action::Archaeology(Act::ShowChange(h, s)) if h == "b1ffff" && s == "b1")));
    assert!(
        acts.iter()
            .any(|a| matches!(a, crate::repo::Action::Archaeology(Act::TestPair(1, 0))))
    );
    Ok(())
}

/// A stash popped with conflicts (no operation) has no other branch: no strip, nothing read.
#[test]
fn a_conflict_from_no_branch_shows_no_strip() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    type State = (RepoTab, Vec<crate::repo::Action>, bool);
    let dir = std::env::temp_dir().join(format!("gitcraft-arch-stash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("f"), "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n")
        .map_err(|e| e.to_string())?;
    let ed = ConflictEditor::open(&dir, "f", None).ok_or("not opened")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
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
    assert!(
        harness
            .query_by_label("HOW THIS CONFLICT CAME ABOUT")
            .is_none()
    );
    assert!(
        !harness
            .state()
            .1
            .iter()
            .any(|a| matches!(a, crate::repo::Action::Archaeology(_)))
    );
    Ok(())
}
