use super::*;

#[test]
fn a_read_of_the_parked_list_from_before_a_change_here_is_dropped() {
    let ctx = egui::Context::default();
    let dir = std::env::temp_dir().join("gitcraft-parked-stale-read");
    let mut tab = RepoTab::new_deferred(dir, &ctx);
    let parked = Parked {
        id: "p1".into(),
        ..Parked::default()
    };
    // Put aside while the list was being read: that read doesn't have it.
    tab.resolver.reading_parked = true;
    tab.resolver.parked.push(parked.clone());
    tab.resolver.parked_changed();
    tab.parked_read(Ok(Vec::new()), &ctx);
    assert_eq!(tab.resolver.parked, vec![parked]);
    assert!(tab.resolver.reading_parked, "read again");
    assert!(!tab.resolver.read_parked_again);
}

#[test]
fn failed_discovery_keeps_recovery_rows_and_finishes_the_read() {
    for error in ["archive is busy", "archive manifest is corrupt"] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(
            std::env::temp_dir().join("gitcraft-parked-failed-discovery"),
            &ctx,
        );
        let parked = Parked {
            id: "recoverable".into(),
            ..Parked::default()
        };
        tab.resolver.parked.push(parked.clone());
        tab.resolver.reading_parked = true;
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.parked_read(Err(error.into()), &ctx);
        });
        assert!(jobs.is_empty(), "failed discovery must not load a kept run");
        assert_eq!(tab.resolver.parked, vec![parked]);
        assert!(!tab.resolver.reading_parked);
        assert!(tab.status.is_some());
        // A later successful empty snapshot must still remove obsolete rows.
        tab.parked_read(Ok(Vec::new()), &ctx);
        assert!(tab.resolver.parked.is_empty());
    }
}

#[test]
fn a_run_read_back_for_a_merge_no_longer_parked_is_left_out() {
    let ctx = egui::Context::default();
    let dir = std::env::temp_dir().join("gitcraft-parked-gone-run");
    let mut tab = RepoTab::new_deferred(dir, &ctx);
    let mut run = super::super::look_tests::run(Phase::Ready, Instant::now());
    run.parked = Some(Parked {
        id: run.id.clone(),
        ..Parked::default()
    });
    // Brought back (or dropped) while its run was read.
    tab.kept_run_read(Some(run));
    assert!(tab.resolver.run.is_none());
}

#[test]
fn a_run_in_a_worktree_parked_behind_the_one_shown_is_found() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(std::env::temp_dir().join("gitcraft-owner-a"), &ctx);
    let mut behind = RepoTab::new_deferred(std::env::temp_dir().join("gitcraft-owner-b"), &ctx);
    behind.resolver.run = Some(super::super::look_tests::run(Phase::Ready, Instant::now()));
    tab.parked.push(Box::new(behind));
    assert!(!tab.resolver_owns("look"));
    assert!(
        tab.resolver_owner("look")
            .is_some_and(|t| t.path.ends_with("gitcraft-owner-b"))
    );
    assert!(tab.resolver_owner("other").is_none());
}

#[test]
fn a_run_record_gives_its_progress_back() -> Result<(), String> {
    let record = RunRecord {
        engine: "Codex".into(),
        plan: vec![PlannedStep {
            id: "a.rs".into(),
            title: "Resolve a.rs".into(),
            file: Some("a.rs".into()),
        }],
        steps: vec![(
            "a.rs".into(),
            Some(StepStatus::Done),
            Some("kept both".into()),
        )],
        finished: Some((true, "all done".into())),
        ..RunRecord::default()
    };
    let text = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    let back: RunRecord = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let p = back.progress();
    assert_eq!(p.steps.len(), 1);
    assert_eq!(p.steps[0].status, StepStatus::Done);
    assert_eq!(p.steps[0].note.as_deref(), Some("kept both"));
    assert!(
        p.finished
            .as_ref()
            .is_some_and(|f| f.ok && f.summary == "all done")
    );
    Ok(())
}
