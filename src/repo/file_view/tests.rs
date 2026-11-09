//! Synthetic replies: no repository reads or worker processes are needed for these races.
use super::*;
use crate::repo::Msg;

fn tab() -> RepoTab {
    RepoTab::new_deferred(
        "/nonexistent/file-view-generation-test".into(),
        &egui::Context::default(),
    )
}

fn view(rev: Option<&str>) -> FileView {
    FileView::new("same.txt", rev.map(str::to_owned), FileTab::History)
}

fn replies(view: &FileView, label: &str) -> [Msg; 3] {
    let (g, p, r) = (view.generation, view.path.clone(), view.rev.clone());
    [
        Msg::FileHistory(g, p.clone(), r.clone(), Err(label.into())),
        Msg::Blame(
            view.blame_generation,
            p.clone(),
            r.clone(),
            Err(label.into()),
        ),
        Msg::FileText(g, p, r, Err(label.into())),
    ]
}

fn deliver(tab: &mut RepoTab, messages: [Msg; 3]) {
    for msg in messages {
        tab.handle(msg, &egui::Context::default());
    }
}

fn assert_label(view: &FileView, label: &str) {
    assert!(matches!(&view.history, Some(Err(error)) if error == label));
    assert!(matches!(&view.blame, Some(Err(error)) if error == label));
    assert!(matches!(&view.text, Some(Err(error)) if error == label));
}

#[test]
fn mutable_refresh_clears_errors_and_rejects_stale_replies() -> Result<(), String> {
    for rev in [
        None,
        Some(""),
        Some("HEAD"),
        Some("main"),
        Some("v1"),
        Some("abcdef0"),
    ] {
        let mut tab = tab();
        tab.file_view = Some(view(rev));
        let old = replies(tab.file_view.as_ref().ok_or("missing file view")?, "old");
        deliver(&mut tab, replies(&view(rev), "unrelated"));
        let cached = replies(
            tab.file_view.as_ref().ok_or("missing file view")?,
            "cached error",
        );
        deliver(&mut tab, cached);
        let v = tab.file_view.as_mut().ok_or("missing file view")?;
        assert!(v.invalidate_mutable());
        assert!(v.needs(FileTab::History) && v.needs(FileTab::Blame) && v.needs(FileTab::File));
        let fresh = replies(v, "fresh");
        deliver(&mut tab, fresh);
        deliver(&mut tab, old);
        assert_label(tab.file_view.as_ref().ok_or("missing file view")?, "fresh");
    }
    Ok(())
}

#[test]
fn close_reopen_and_a_b_a_reject_previous_openings() -> Result<(), String> {
    for via_b in [false, true] {
        let mut tab = tab();
        tab.file_view = Some(view(Some("HEAD")));
        let old = replies(tab.file_view.as_ref().ok_or("missing file view")?, "old A");
        tab.file_view = None;
        if via_b {
            let mut b = view(Some("HEAD"));
            b.path = "other.txt".into();
            tab.file_view = Some(b);
        }
        tab.file_view = Some(view(Some("HEAD")));
        deliver(&mut tab, old);
        let v = tab.file_view.as_ref().ok_or("missing file view")?;
        assert!(v.history.is_none() && v.blame.is_none() && v.text.is_none());
    }
    Ok(())
}

#[test]
fn ordinary_pane_switches_and_parallel_tabs_preserve_their_own_replies() -> Result<(), String> {
    let mut a = tab();
    let mut b = tab();
    a.file_view = Some(view(None));
    b.file_view = Some(view(None));
    let a_reply = replies(a.file_view.as_ref().ok_or("missing file view")?, "A");
    let b_reply = replies(b.file_view.as_ref().ok_or("missing file view")?, "B");
    let generation = a.file_view.as_ref().ok_or("missing file view")?.generation;
    // Preload panes so switching needs no background IO.
    deliver(
        &mut a,
        replies(
            b.file_view.as_ref().ok_or("missing file view")?,
            "wrong tab",
        ),
    );
    let cached = replies(a.file_view.as_ref().ok_or("missing file view")?, "cached");
    deliver(&mut a, cached);
    for pane in [FileTab::Blame, FileTab::File, FileTab::History] {
        a.open_file_view("same.txt".into(), None, pane, &egui::Context::default());
        assert_eq!(
            a.file_view.as_ref().ok_or("missing file view")?.generation,
            generation
        );
    }
    deliver(&mut a, a_reply);
    deliver(&mut b, b_reply);
    assert_label(a.file_view.as_ref().ok_or("missing file view")?, "A");
    assert_label(b.file_view.as_ref().ok_or("missing file view")?, "B");
    Ok(())
}

#[test]
fn full_object_ids_retain_their_cache_and_generation() {
    for len in [40, 64] {
        let mut v = view(Some(&"a".repeat(len)));
        v.history = Some(Ok(Vec::new()));
        let generation = v.generation;
        assert!(!v.invalidate_mutable());
        assert_eq!(v.generation, generation);
        assert!(v.history.is_some());
    }
}

#[test]
fn line_history_replies_cannot_cross_refresh_or_identical_reopening() -> Result<(), String> {
    for reopen in [false, true] {
        let mut tab = tab();
        let mut v = view(None);
        v.lines = Some((git::LinesAt::Index, (4, 9)));
        let old =
            line_history::history_reply(v.generation, v.path.clone(), git::LinesAt::Index, (4, 9));
        tab.file_view = Some(v);
        if reopen {
            let mut v = view(None);
            v.lines = Some((git::LinesAt::Index, (4, 9)));
            tab.file_view = Some(v);
        } else {
            tab.file_view
                .as_mut()
                .ok_or("missing file view")?
                .invalidate_mutable();
        }
        tab.handle(
            Msg::Then(Box::new(move |tab, ctx| old(tab, Err("stale".into()), ctx))),
            &egui::Context::default(),
        );
        let v = tab.file_view.as_ref().ok_or("missing file view")?;
        assert!(v.history.is_none());
        assert_eq!(v.lines, Some((git::LinesAt::Index, (4, 9))));
        let fresh =
            line_history::history_reply(v.generation, v.path.clone(), git::LinesAt::Index, (4, 9));
        tab.handle(
            Msg::Then(Box::new(move |tab, ctx| fresh(tab, Ok(Vec::new()), ctx))),
            &egui::Context::default(),
        );
        assert!(matches!(
            tab.file_view.as_ref().and_then(|v| v.history.as_ref()),
            Some(Ok(_))
        ));
    }
    Ok(())
}

#[test]
fn blame_remains_visible_until_current_refresh_replaces_it() -> Result<(), String> {
    let mut tab = tab();
    let mut v = view(None);
    let old = replies(&v, "stale failure");
    v.blame = Some(Ok(BlameData::default()));
    assert!(v.invalidate_mutable());
    assert!(matches!(v.blame, Some(Ok(_))));
    assert!(v.needs(FileTab::Blame));
    let fresh = replies(&v, "current failure");
    tab.file_view = Some(v);
    deliver(&mut tab, old);
    let v = tab.file_view.as_ref().ok_or("missing view")?;
    assert!(matches!(v.blame, Some(Ok(_))));
    assert!(v.needs(FileTab::Blame));
    deliver(&mut tab, fresh);
    let v = tab.file_view.as_ref().ok_or("missing view")?;
    assert!(matches!(&v.blame, Some(Err(e)) if e == "current failure"));
    assert!(!v.needs(FileTab::Blame));
    Ok(())
}

#[test]
fn unchanged_line_inputs_retry_failed_history_and_only_reuse_success() {
    let mut v = view(None);
    let key = ("a".repeat(40), "same.txt".to_owned(), (4, 9));
    v.line_inputs = Some(key.clone());
    v.history = Some(Err("transient connection failure".into()));
    assert!(!v.reuses_line_inputs(&Ok(key.clone())));
    v.history = Some(Ok(Vec::new()));
    assert!(v.reuses_line_inputs(&Ok(key.clone())));
    assert!(!v.reuses_line_inputs(&Err("probe failed".into())));
    let changed = ("b".repeat(40), key.1, key.2);
    assert!(!v.reuses_line_inputs(&Ok(changed)));
}

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
    assert!(matches!(&reply, Msg::Blame(_, _, _, Ok(_))));
    tab.handle(reply, &ctx);
    assert!(
        !tab.file_view
            .as_ref()
            .ok_or("missing view")?
            .needs(FileTab::Blame)
    );
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}
