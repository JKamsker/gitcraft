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
fn previous_run_uses_workflow_sequence_when_timestamps_tie_and_unknown_order_stays_unknown()
-> Result<(), String> {
    let mut current = info(10, 2, 100);
    current.number = 50;
    let mut future = info(11, 1, 100);
    future.number = 51;
    let mut previous = info(9, 1, 100);
    previous.number = 49;
    assert!(!earlier(&future, &current));
    assert!(earlier(&previous, &current));
    assert_eq!(
        baseline_key(
            &current,
            Baseline::Previous,
            &[],
            &[future, current.clone(), previous]
        )?,
        Some(RunKey { run: 9, attempt: 1 })
    );
    let unknown = RunInfo {
        id: 8,
        ..RunInfo::default()
    };
    assert!(!earlier(&unknown, &current));
    let tied_unknown = info(7, 1, 100);
    assert!(!earlier(&tied_unknown, &current));
    assert_eq!(
        baseline_key(&current, Baseline::Previous, &[], &[unknown, tied_unknown])?,
        None
    );
    Ok(())
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
