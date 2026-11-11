use super::*;
#[test]
fn pure_bakeoff_recorded_launch_failure_is_retryable_and_stale_launch_receipts_are_rejected()
-> Result<(), String> {
    let mut view = Bakeoff::new(
        vec![Attempt {
            name: "a".into(),
            rev: "a".into(),
            worktree: None,
        }],
        String::new(),
    );
    let status = PathBuf::from("owned-unique-receipt");
    view.statuses[0] = Some(status.clone());
    view.builds[0] = Some(Built::Preparing);
    view.accept_launch(view.request + 1, 0, &status, Err("old launch".into()));
    assert_eq!(view.builds[0], Some(Built::Preparing));
    view.accept_launch(
        view.request,
        0,
        &PathBuf::from("previous-launch"),
        Err("stale path".into()),
    );
    assert_eq!(view.builds[0], Some(Built::Preparing));
    view.accept_launch(
        view.request,
        0,
        &status,
        Err("recorded PTY spawn failure".into()),
    );
    assert_eq!(view.builds[0], Some(Built::Unavailable));
    assert!(
        view.note
            .as_ref()
            .ok_or("expected Some")?
            .0
            .contains("spawn failure")
    );
    // Retry owns a different status path; no receipt from the old run wins.
    view.statuses[0] = Some(PathBuf::from("fresh-owned-receipt"));
    view.builds[0] = Some(Built::Preparing);
    view.accept_builds(view.request, vec![(0, status, Built::Passed)]);
    assert_eq!(view.builds[0], Some(Built::Preparing));
    Ok(())
}
#[test]
fn pure_bakeoff_closed_or_reloaded_comparisons_reject_old_build_statuses() {
    let attempts = vec![
        Attempt {
            name: "a".into(),
            rev: "a".into(),
            worktree: None,
        },
        Attempt {
            name: "b".into(),
            rev: "b".into(),
            worktree: None,
        },
    ];
    let old = Bakeoff::new(attempts.clone(), String::new());
    let mut current = Bakeoff::new(attempts, String::new());
    assert_ne!(old.request, current.request);
    let path = PathBuf::from("new-owned-status");
    current.builds[0] = Some(Built::Running);
    current.statuses[0] = Some(path.clone());
    current.accept_builds(old.request, vec![(0, path.clone(), Built::Passed)]);
    assert_eq!(current.builds[0], Some(Built::Running));
    current.accept_builds(
        current.request,
        vec![(0, PathBuf::from("old-owned-status"), Built::Passed)],
    );
    assert_eq!(current.builds[0], Some(Built::Running));
    current.accept_builds(current.request, vec![(0, path, Built::Passed)]);
    assert_eq!(current.builds[0], Some(Built::Passed));
}
