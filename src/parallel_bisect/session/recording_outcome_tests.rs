use super::*;
#[test]
fn cancelled_setup_ownership_error_survives_without_workspace() {
    let stopped = AtomicBool::new(true);
    let error = "could not finish owned process: recorded failure";
    assert!(stopped.load(Ordering::Relaxed));
    assert_eq!(
        prepared_outcome(Err(error.into())),
        Outcome::Failed(error.into())
    );
}
#[test]
fn cancelled_worker_cleanup_failure_survives_final_normalization() {
    let mut answered = false;
    let mut ended = None;
    assert!(
        record_round_result(
            &mut ended,
            Err("missing cleaned receipt".into()),
            true,
            |_| {
                answered = true;
                Err("failed worker has no answer".into())
            }
        )
        .is_none()
    );
    assert_eq!(
        finish_round(ended, true),
        Err("missing cleaned receipt".into())
    );
    assert!(!answered, "unexpected answer callback");
}
#[test]
fn selected_answer_keeps_late_worker_error_as_concrete_warning() {
    let mut answered = false;
    let selected = Outcome::FirstBad("a".repeat(40));
    let mut ended = Some(Ok(selected.clone()));
    let warning = record_round_result(
        &mut ended,
        Err("owner stop acknowledgement missing".into()),
        true,
        |_| {
            answered = true;
            Err("late worker cannot replace answer".into())
        },
    );
    assert_eq!(
        warning.as_deref(),
        Some("owner stop acknowledgement missing")
    );
    assert_eq!(finish_round(ended, true), Ok(Some(selected)));
    assert!(!answered, "unexpected answer callback");
}
#[test]
fn confirmed_normal_cancellation_remains_cancelled() {
    let mut answered = false;
    assert_eq!(prepared_outcome(Ok(Outcome::Cancelled)), Outcome::Cancelled);
    let mut ended = None;
    assert!(
        record_round_result(
            &mut ended,
            Ok(Ran {
                ended: super::super::verdict::Ended::Stopped,
                output: String::new()
            }),
            true,
            |_| {
                answered = true;
                Err("cancelled successful worker is ignored".into())
            }
        )
        .is_none()
    );
    assert_eq!(finish_round(ended, true), Ok(Some(Outcome::Cancelled)));
    assert!(!answered, "unexpected answer callback");
}
