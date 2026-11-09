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

#[test]
fn status_probes_preserve_inflight_inputs_and_reject_changed_or_older_observations()
-> Result<(), String> {
    let ctx = egui::Context::default();
    for pane in [FileTab::History, FileTab::File] {
        let mut tab = tab();
        let mut v = view(None);
        v.tab = pane;
        let generation = v.generation;
        tab.file_view = Some(v);
        let inputs = FileInputs {
            history: Some("commit-a".into()),
            text: Some(content_key(b"same")),
            ..Default::default()
        };
        let mut latest_check = 0;
        for _ in 0..3 {
            let (_, jobs) =
                crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
            assert_eq!(
                jobs.len(),
                1,
                "probe only; the current read is not resubmitted"
            );
            let v = tab.file_view.as_ref().ok_or("missing view")?;
            assert_eq!(v.generation, generation);
            latest_check = v.input_check;
            let blame_generation = v.blame_generation;
            tab.finish_file_probe(
                generation,
                blame_generation,
                latest_check,
                Ok(ProbeInputs::File(inputs.clone())),
                &ctx,
            );
        }
        let input = match pane {
            FileTab::History => inputs.history.clone(),
            _ => inputs.text.clone(),
        };
        assert!(tab.accept_file_inputs(generation, "same.txt", None, pane, input, &ctx));
        let reply = match pane {
            FileTab::History => {
                Msg::FileHistory(generation, "same.txt".into(), None, Ok(Vec::new()))
            }
            _ => Msg::FileText(
                generation,
                "same.txt".into(),
                None,
                Ok(FileText::new(b"same", "same.txt", false)),
            ),
        };
        tab.handle(reply, &ctx);
        let (_, jobs) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
        assert_eq!(jobs.len(), 1);
        let current_check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
        let changed = FileInputs {
            history: Some("commit-b".into()),
            text: Some(content_key(b"changed")),
            ..Default::default()
        };
        tab.file_status_inputs(generation, latest_check, changed.clone(), &ctx);
        assert_eq!(
            tab.file_view.as_ref().ok_or("missing view")?.generation,
            generation
        );
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.file_status_inputs(generation, current_check, changed, &ctx)
        });
        assert_eq!(
            jobs.len(),
            1,
            "changed relevant input requests a replacement"
        );
        assert_ne!(
            tab.file_view.as_ref().ok_or("missing view")?.generation,
            generation
        );
        assert!(!tab.accept_file_inputs(
            generation,
            "same.txt",
            None,
            pane,
            Some("stale".into()),
            &ctx
        ));
    }
    // A status observation can arrive before a delayed initial result. Its captured
    // input must agree; otherwise even the current generation's result is superseded.
    let mut tab = tab();
    tab.file_view = Some(view(None));
    let generation = tab.file_view.as_ref().ok_or("missing view")?.generation;
    let (_, _) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
    let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
    tab.file_status_inputs(
        generation,
        check,
        FileInputs {
            history: Some("new".into()),
            text: None,
            ..Default::default()
        },
        &ctx,
    );
    let (accepted, jobs) = crate::repo::messages::record_jobs(|| {
        tab.accept_file_inputs(
            generation,
            "same.txt",
            None,
            FileTab::History,
            Some("old".into()),
            &ctx,
        )
    });
    assert!(!accepted);
    assert_eq!(jobs.len(), 1);
    Ok(())
}

#[test]
fn actual_file_loaders_finish_across_unrelated_status_changes() -> Result<(), String> {
    let ctx = egui::Context::default();
    for pane in [FileTab::History, FileTab::File] {
        let dir = crate::repo::revert_tests::scratch(
            if pane == FileTab::History {
                "history-inflight-probe"
            } else {
                "file-inflight-probe"
            },
            &[
                ("same.txt", "selected bytes\n"),
                ("other.txt", "unrelated\n"),
            ],
        )?;
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let mut v = view(None);
        v.tab = pane;
        let generation = v.generation;
        tab.file_view = Some(v);
        let (_, mut jobs) = crate::repo::messages::record_jobs(|| tab.load_file_view(&ctx));
        let delayed_reply = jobs.pop().ok_or("missing actual loader")?();
        std::fs::write(dir.join("other.txt"), "unrelated edit\n").map_err(|e| e.to_string())?;
        let (_, mut jobs) =
            crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
        let probe = jobs.pop().ok_or("missing status probe")?();
        tab.handle(probe, &ctx);
        tab.handle(delayed_reply, &ctx);
        let v = tab.file_view.as_ref().ok_or("missing view")?;
        assert_eq!(v.generation, generation);
        assert!(!v.needs(pane));
        match pane {
            FileTab::History => assert_eq!(
                v.history_input.as_deref(),
                Some(git::resolve(&dir, "HEAD")?.as_str())
            ),
            _ => assert_eq!(v.text_input, Some(content_key(b"selected bytes\n"))),
        }
        if pane == FileTab::File {
            std::fs::write(dir.join("same.txt"), "selected new bytes\n")
                .map_err(|e| e.to_string())?;
            let (_, mut jobs) =
                crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
            let probe = jobs.pop().ok_or("missing changed probe")?();
            let (_, mut replacements) =
                crate::repo::messages::record_jobs(|| tab.handle(probe, &ctx));
            assert_eq!(replacements.len(), 1);
            let replacement = replacements.pop().ok_or("missing replacement loader")?();
            tab.handle(replacement, &ctx);
            let v = tab.file_view.as_ref().ok_or("missing view")?;
            assert_ne!(v.generation, generation);
            assert_eq!(v.text_input, Some(content_key(b"selected new bytes\n")));
            assert!(!v.needs(FileTab::File));
        }
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn status_probes_coalesce_followups_and_retry_busy_without_another_status() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab();
    tab.file_view = Some(view(None));
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        for _ in 0..20 {
            tab.status_changed_file_view(&ctx);
        }
    });
    assert_eq!(jobs.len(), 1);
    let v = tab.file_view.as_ref().ok_or("missing view")?;
    let (generation, blame, check) = (v.generation, v.blame_generation, v.input_check);
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        tab.finish_file_probe(
            generation,
            blame,
            check,
            Ok(ProbeInputs::File(FileInputs::default())),
            &ctx,
        )
    });
    assert_eq!(jobs.len(), 1, "exactly one dirty follow-up");
    let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        tab.finish_file_probe(
            generation,
            blame,
            check,
            Err(crate::tasks::BUSY.into()),
            &ctx,
        )
    });
    assert!(jobs.is_empty());
    let v = tab.file_view.as_mut().ok_or("missing view")?;
    assert!(v.probe.active.is_none() && v.probe.dirty && v.probe.retry.is_some());
    v.probe.retry = Some(Instant::now());
    let (_, jobs) = crate::repo::messages::record_jobs(|| tab.poll_file_view(&ctx));
    assert_eq!(
        jobs.len(),
        1,
        "timer retries without another filesystem event"
    );
    for error in ["cancelled", "panicked", "Background work has shut down"] {
        let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.finish_file_probe(generation, blame, check, Err(error.into()), &ctx)
        });
        assert!(jobs.is_empty());
        let v = tab.file_view.as_ref().ok_or("missing view")?;
        assert!(v.probe.active.is_none() && !v.probe.dirty);
        let (_, _) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
    }
    Ok(())
}

#[test]
fn pending_line_history_accepts_unchanged_observations_in_both_orders() -> Result<(), String> {
    let ctx = egui::Context::default();
    let key = ("a".repeat(40), "same.txt".to_owned(), (4, 9));
    for probe_first in [false, true] {
        let mut tab = tab();
        let mut v = view(None);
        v.lines = Some((git::LinesAt::WorkTree, (4, 9)));
        let (generation, blame) = (v.generation, v.blame_generation);
        tab.file_view = Some(v);
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.load_line_history(&ctx);
            tab.load_line_history(&ctx);
        });
        assert_eq!(jobs.len(), 1, "pending loader is not duplicated");
        let token =
            tab.file_view.as_ref().ok_or("missing view")?.pending[0].ok_or("missing loader")?;
        let (_, _) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
        let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
        if probe_first {
            tab.finish_file_probe(
                generation,
                blame,
                check,
                Ok(ProbeInputs::Lines(Ok(key.clone()))),
                &ctx,
            );
        }
        tab.finish_line_history(
            generation,
            token,
            Ok((Some(key.clone()), Ok(Vec::new()))),
            &ctx,
        );
        if !probe_first {
            tab.finish_file_probe(
                generation,
                blame,
                check,
                Ok(ProbeInputs::Lines(Ok(key.clone()))),
                &ctx,
            );
        }
        let v = tab.file_view.as_ref().ok_or("missing view")?;
        assert_eq!(v.generation, generation);
        assert!(matches!(v.history, Some(Ok(_))));
        // A completed failure with the same key must be retried by the next status.
        tab.file_view.as_mut().ok_or("missing view")?.history = Some(Err("transient read".into()));
        let (_, _) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
        let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.finish_file_probe(
                generation,
                blame,
                check,
                Ok(ProbeInputs::Lines(Ok(key.clone()))),
                &ctx,
            )
        });
        assert_eq!(jobs.len(), 1);
        assert_ne!(
            tab.file_view.as_ref().ok_or("missing view")?.generation,
            generation
        );
    }
    Ok(())
}

#[test]
fn pinned_pending_blame_survives_status_but_changed_dependencies_and_refresh_supersede_it()
-> Result<(), String> {
    let ctx = egui::Context::default();
    for len in [40, 64] {
        let mut tab = tab();
        let mut v = view(Some(&"a".repeat(len)));
        v.tab = FileTab::Blame;
        let generation = v.generation;
        let blame = v.blame_generation;
        tab.file_view = Some(v);
        let (_, jobs) = crate::repo::messages::record_jobs(|| tab.load_file_view(&ctx));
        assert_eq!(jobs.len(), 1);
        let token = tab.file_view.as_ref().ok_or("missing view")?.pending[1]
            .ok_or("missing pending blame")?;
        let (_, _) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
        let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
        tab.finish_file_probe(
            generation,
            blame,
            check,
            Ok(ProbeInputs::File(FileInputs {
                blame: Some("dependencies-a".into()),
                ..Default::default()
            })),
            &ctx,
        );
        assert_eq!(
            tab.file_view
                .as_ref()
                .ok_or("missing view")?
                .blame_generation,
            blame
        );
        assert!(tab.finish_file_load(FileTab::Blame, token, blame, None, &ctx));
        assert!(tab.accept_blame_input(Some("dependencies-a".into()), &ctx));
        let v = tab.file_view.as_ref().ok_or("missing view")?;
        tab.handle(
            Msg::Blame(
                blame,
                v.path.clone(),
                v.rev.clone(),
                Ok(BlameData::default()),
            ),
            &ctx,
        );
        let (_, _) = crate::repo::messages::record_jobs(|| tab.status_changed_file_view(&ctx));
        let check = tab.file_view.as_ref().ok_or("missing view")?.input_check;
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.finish_file_probe(
                generation,
                blame,
                check,
                Ok(ProbeInputs::File(FileInputs {
                    blame: Some("dependencies-b".into()),
                    ..Default::default()
                })),
                &ctx,
            )
        });
        assert_eq!(jobs.len(), 1);
        let newer = tab
            .file_view
            .as_ref()
            .ok_or("missing view")?
            .blame_generation;
        assert_ne!(newer, blame);
        let replacement = tab.file_view.as_ref().ok_or("missing view")?.pending[1]
            .ok_or("missing replacement blame")?;
        assert_ne!(replacement, token);
        let (_, jobs) =
            crate::repo::messages::record_jobs(|| tab.invalidate_mutable_file_view(&ctx));
        assert!(
            jobs.is_empty(),
            "superseded loader retains its slot until completion"
        );
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            assert!(!tab.finish_file_load(FileTab::Blame, replacement, newer, None, &ctx));
        });
        assert_eq!(jobs.len(), 1, "retired epoch submits a fresh replacement");
        assert_ne!(
            tab.file_view.as_ref().ok_or("missing view")?.pending[1],
            Some(replacement)
        );
    }
    Ok(())
}

#[test]
fn absolute_attribution_dependencies_keep_repository_routing_and_follow_file_reads()
-> Result<(), String> {
    let repo = std::path::Path::new("ssh://recorded/home/repo");
    let value =
        attribution_file_input_with(repo, "/home/user/ignore'file", |actual_repo, script| {
            assert_eq!(
                actual_repo, repo,
                "the remote repository is never replaced by an absolute local path"
            );
            assert!(script.contains(&crate::remote::sh_quote("/home/user/ignore'file")));
            assert!(script.contains("git hash-object --stdin <"));
            assert!(
                !script.contains("readlink"),
                "Git follows configured file symlinks"
            );
            Ok(git::ShellRun {
                code: 0,
                stdout: b"file:fingerprint".to_vec(),
                stderr: Vec::new(),
            })
        })?;
    assert_eq!(value, b"file:fingerprint");
    Ok(())
}

#[cfg(unix)]
#[test]
fn blame_key_follows_configured_symlink_target_contents() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let dir =
        crate::repo::revert_tests::scratch("blame-key-symlink", &[("same.txt", "fixture\n")])?;
    let target = dir.join("ignored-target");
    std::fs::write(&target, b"# first\n").map_err(|e| e.to_string())?;
    symlink(&target, dir.join("ignore-link")).map_err(|e| e.to_string())?;
    git::run(
        Some(&dir),
        &["config", "blame.ignoreRevsFile", "ignore-link"],
    )?;
    let rev = git::resolve(&dir, "HEAD")?;
    let before = blame_input(&dir, Some(&rev), "same.txt").ok_or("missing initial key")?;
    std::fs::write(&target, b"# changed target, unchanged link\n").map_err(|e| e.to_string())?;
    assert_ne!(
        blame_input(&dir, Some(&rev), "same.txt").ok_or("missing changed key")?,
        before
    );
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn blame_key_observes_effective_index_attributes_and_moving_mailmap_blob() -> Result<(), String> {
    let dir = crate::repo::revert_tests::scratch(
        "blame-key-attributes-mailmap",
        &[("same.txt", "fixture\n")],
    )?;
    let viewed = git::resolve(&dir, "HEAD")?;
    for driver in ["first", "second"] {
        std::fs::write(
            dir.join(".gitattributes"),
            format!("same.txt diff={driver}\n"),
        )
        .map_err(|e| e.to_string())?;
        git::run(Some(&dir), &["add", ".gitattributes"])?;
        std::fs::remove_file(dir.join(".gitattributes")).map_err(|e| e.to_string())?;
        let attributes = git::run(Some(&dir), &["check-attr", "-z", "--all", "--", "same.txt"])?;
        assert!(attributes.contains(driver), "Git must use index fallback");
        let key = blame_input(&dir, Some(&viewed), "same.txt").ok_or("missing attribute key")?;
        if driver == "first" {
            std::fs::write(dir.join("first-key"), key).map_err(|e| e.to_string())?;
        } else {
            assert_ne!(
                key,
                std::fs::read_to_string(dir.join("first-key")).map_err(|e| e.to_string())?
            );
        }
    }
    git::run(Some(&dir), &["reset", "--hard", "HEAD"])?;
    git::run(Some(&dir), &["config", "mailmap.blob", "HEAD:.mailmap"])?;
    let email = git::run(Some(&dir), &["show", "-s", "--format=%ae", "HEAD"])?;
    let mut before = None;
    for (name, email_to) in [
        ("First", "first@example.invalid"),
        ("Second", "second@example.invalid"),
    ] {
        std::fs::write(
            dir.join(".mailmap"),
            format!("{name} <{email_to}> <{}>\n", email.trim()),
        )
        .map_err(|e| e.to_string())?;
        git::run(Some(&dir), &["add", ".mailmap"])?;
        git::run(Some(&dir), &["commit", "-qm", name])?;
        std::fs::remove_file(dir.join(".mailmap")).map_err(|e| e.to_string())?;
        let key = blame_input(&dir, Some(&viewed), "same.txt").ok_or("missing mailmap key")?;
        let attribution = git::blame(&dir, Some(&viewed), "same.txt")?;
        assert_eq!(
            attribution.first().ok_or("missing blame line")?.email,
            email_to
        );
        if let Some(before) = before {
            assert_ne!(key, before);
        }
        before = Some(key);
    }
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}
