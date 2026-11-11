use super::*;

fn info(id: u64, attempt: u64, created: i64) -> RunInfo {
    RunInfo {
        id,
        attempt,
        created: Some(created),
        ..RunInfo::default()
    }
}

#[test]
fn shortcuts_resolve_to_exact_run_attempts_and_keep_reruns_separate() -> Result<(), String> {
    let current = info(10, 3, 100);
    let green = vec![info(7, 2, 70)];
    let recent = vec![
        info(11, 1, 110),
        info(10, 3, 100),
        info(9, 4, 90),
        info(8, 1, 80),
    ];
    assert_eq!(
        baseline_key(&current, Baseline::Latest, &green, &recent)?,
        Some(RunKey { run: 7, attempt: 2 })
    );
    assert_eq!(
        baseline_key(&current, Baseline::Previous, &green, &recent)?,
        Some(RunKey { run: 9, attempt: 4 })
    );
    assert_eq!(
        baseline_key(&current, Baseline::PreviousAttempt, &green, &recent)?,
        Some(RunKey {
            run: 10,
            attempt: 2
        })
    );
    let pinned = RunKey { run: 7, attempt: 2 };
    assert_eq!(
        baseline_key(&current, Baseline::Attempt(pinned), &[], &[])?,
        Some(pinned)
    );
    assert!(
        baseline_key(
            &info(10, 1, 100),
            Baseline::PreviousAttempt,
            &green,
            &recent
        )
        .is_err()
    );
    assert!(
        baseline_key(
            &current,
            Baseline::Attempt(RunKey::of(&current)),
            &green,
            &recent
        )
        .is_err()
    );
    assert!(baseline_key(&current, Baseline::Run(999), &green, &recent).is_err());
    Ok(())
}
