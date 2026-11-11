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
