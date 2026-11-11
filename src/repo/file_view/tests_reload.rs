#[test]
fn failed_reload_invalidates_mutable_views_and_submits_replacement_reads() -> Result<(), String> {
    let ctx = egui::Context::default();
    for pane in [FileTab::History, FileTab::File, FileTab::Blame] {
        for successful_blame in [false, true] {
            let mut tab = tab();
            let mut v = view(Some("HEAD"));
            v.tab = pane;
            v.history = Some(Ok(Vec::new()));
            v.text = Some(Err("cached text error".into()));
            v.blame = Some(if successful_blame {
                Ok(BlameData::default())
            } else {
                Err("cached blame error".into())
            });
            let old = replies(&v, "old reply");
            tab.file_view = Some(v);
            let (_, jobs) = crate::repo::messages::record_jobs(|| {
                tab.handle(Msg::Loaded(Err("reload failed".into())), &ctx)
            });
            // Exercises the real Loaded handler and loader, without running a worker.
            assert_eq!(jobs.len(), 1);
            assert_eq!(tab.load_error.as_deref(), Some("reload failed"));
            let v = tab.file_view.as_ref().ok_or("missing view")?;
            assert!(v.history.is_none() && v.text.is_none());
            assert_eq!(matches!(v.blame, Some(Ok(_))), successful_blame);
            assert!(v.needs(FileTab::Blame));
            deliver(&mut tab, old);
            let v = tab.file_view.as_ref().ok_or("missing view")?;
            assert!(v.history.is_none() && v.text.is_none());
            let current = replies(v, "current reply");
            deliver(&mut tab, current);
            assert_label(
                tab.file_view.as_ref().ok_or("missing view")?,
                "current reply",
            );
        }
    }
    let mut tab = tab();
    let mut v = view(Some(&"a".repeat(40)));
    v.history = Some(Ok(Vec::new()));
    let generation = v.generation;
    tab.file_view = Some(v);
    let (_, jobs) =
        crate::repo::messages::record_jobs(|| tab.handle(Msg::Loaded(Err("failed".into())), &ctx));
    assert!(jobs.is_empty());
    let v = tab.file_view.as_ref().ok_or("missing view")?;
    assert_eq!(v.generation, generation);
    assert!(v.history.is_some());
    Ok(())
}

#[test]
fn fixed_commit_refresh_preserves_content_but_reloads_configuration_sensitive_blame()
-> Result<(), String> {
    let ctx = egui::Context::default();
    for len in [40, 64] {
        for pane in [FileTab::File, FileTab::History] {
            for successful_blame in [false, true] {
                let mut tab = tab();
                let mut v = view(Some(&"a".repeat(len)));
                v.tab = pane;
                v.history = Some(Ok(Vec::new()));
                v.text = Some(Err("retained immutable text".into()));
                v.blame = Some(if successful_blame {
                    Ok(BlameData::default())
                } else {
                    Err("retry configuration".into())
                });
                let generation = v.generation;
                let old = replies(&v, "old");
                tab.file_view = Some(v);
                let (_, jobs) =
                    crate::repo::messages::record_jobs(|| tab.invalidate_mutable_file_view(&ctx));
                assert!(jobs.is_empty());
                let v = tab.file_view.as_ref().ok_or("missing view")?;
                assert_eq!(v.generation, generation);
                assert!(matches!(v.history, Some(Ok(_))));
                assert!(matches!(&v.text, Some(Err(e)) if e == "retained immutable text"));
                assert_eq!(matches!(v.blame, Some(Ok(_))), successful_blame);
                assert!(v.needs(FileTab::Blame));
                let [_, old_blame, _] = old;
                tab.handle(old_blame, &ctx);
                assert!(
                    tab.file_view
                        .as_ref()
                        .ok_or("missing view")?
                        .needs(FileTab::Blame)
                );
                let rev = Some("a".repeat(len));
                let (_, jobs) = crate::repo::messages::record_jobs(|| {
                    tab.open_file_view("same.txt".into(), rev, FileTab::Blame, &ctx)
                });
                assert_eq!(jobs.len(), 1, "selecting dirty blame must submit a read");
                let v = tab.file_view.as_ref().ok_or("missing view")?;
                let current = Msg::Blame(
                    v.blame_generation,
                    v.path.clone(),
                    v.rev.clone(),
                    Ok(BlameData::default()),
                );
                tab.handle(current, &ctx);
                assert!(
                    !tab.file_view
                        .as_ref()
                        .ok_or("missing view")?
                        .needs(FileTab::Blame)
                );
                let (_, jobs) = crate::repo::messages::record_jobs(|| tab.load_file_view(&ctx));
                assert!(
                    jobs.is_empty(),
                    "ordinary pane selection reuses current blame"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn retained_blame_is_replaced_by_the_actual_dispatched_loader() -> Result<(), String> {
    let dir = crate::repo::revert_tests::scratch(
        "blame-dispatch-refresh",
        &[("same.txt", "owned fixture\n")],
    )?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut v = view(None);
    v.tab = FileTab::Blame;
    v.blame = Some(Ok(BlameData::default()));
    tab.file_view = Some(v);
    let (_, mut jobs) =
        crate::repo::messages::record_jobs(|| tab.invalidate_mutable_file_view(&ctx));
    assert_eq!(jobs.len(), 1);
    assert!(matches!(
        tab.file_view.as_ref().and_then(|v| v.blame.as_ref()),
        Some(Ok(_))
    ));
    let reply = jobs.pop().ok_or("missing replacement job")?();
    assert!(matches!(&reply, Msg::Then(_)));
    tab.handle(reply, &ctx);
    assert!(
        !tab.file_view
            .as_ref()
            .ok_or("missing view")?
            .needs(FileTab::Blame)
    );
    let lines = &tab
        .file_view
        .as_ref()
        .ok_or("missing view")?
        .blame
        .as_ref()
        .ok_or("missing blame")?
        .as_ref()
        .map_err(Clone::clone)?
        .lines;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "owned fixture");
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}
