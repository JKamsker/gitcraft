use super::*;
#[test]
fn shared_predispatch_guard_keeps_ancestor_cancellation_sticky() -> Result<(), String> {
    let lease = Cancellation::default();
    let target = lease.child();
    assert_eq!(before_start(target.is_cancelled(), false), None);
    lease.cancel();
    for _ in 0..3 {
        assert_eq!(
            before_start(target.is_cancelled(), false),
            Some("cancelled")
        );
    }
    assert!(matches!(
        checked(Err(before_start(target.is_cancelled(), false)
            .ok_or("missing sticky cancellation")?
            .into())),
        Err(Failure::CancelledBeforeStart)
    ));
    assert_eq!(
        before_start(false, true),
        Some("process timed out and was stopped")
    );
    Ok(())
}
#[test]
fn failed_ack_or_unavailable_cleanup_worker_always_surfaces_uncertainty() {
    for why in ["capacity closed", "worker lost", "wrong receipt"] {
        let mut stopped = Some("cancelled".into());
        let mut confirmed = true;
        record_stop_confirmation(&mut stopped, &mut confirmed, Err(why.into()));
        assert_eq!(
            stop_kind(stopped.as_deref(), confirmed),
            Some(StopKind::Uncertain)
        );
        assert!(
            stopped
                .as_deref()
                .unwrap_or_default()
                .contains("cleanup unconfirmed")
        );
    }
    let mut stopped = Some("cancelled".into());
    let mut confirmed = false;
    record_stop_confirmation(&mut stopped, &mut confirmed, Ok(()));
    assert_eq!(
        stop_kind(stopped.as_deref(), confirmed),
        Some(StopKind::Cancelled)
    );
}
#[test]
fn cancellation_requires_no_dispatch_or_confirmed_cleanup() {
    assert!(matches!(
        checked(Err("cancelled".into())),
        Err(Failure::CancelledBeforeStart)
    ));
    assert!(matches!(
        checked(Err("process timed out and was stopped".into())),
        Err(Failure::TimedOutBeforeStart)
    ));
    assert!(matches!(
        checked(Err("could not finish owned process".into())),
        Err(Failure::Failed(_))
    ));
    assert!(matches!(
        checked(Err(
            "Process service stopped before returning a result".into()
        )),
        Err(Failure::Failed(_))
    ));
    assert_eq!(
        stop_kind(Some("cancelled"), false),
        Some(StopKind::Uncertain)
    );
    assert_eq!(
        stop_kind(Some("cancelled"), true),
        Some(StopKind::Cancelled)
    );
    assert_eq!(
        stop_kind(Some("output delivery incomplete"), false),
        Some(StopKind::Uncertain)
    );
}
