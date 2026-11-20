use super::super::Waiter;
use crate::repo::RepoTab;

/// A tab where `holder` holds the operation slot and Fetch waits for it.
fn held_by(holder: &str, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/busy-op-released".into(), ctx);
    tab.busy_op = Some(holder.to_owned());
    tab.waiting.push_back(Waiter::Fetch);
    tab.status = Some(crate::repo::Notice::plain((
        "Fold: preparing".into(),
        false,
    )));
    tab
}

/// A preparation step that went on to start its operation leaves the waiters to it: nothing
/// starts and nothing is dropped, whatever the step's outcome.
#[test]
fn nothing_starts_or_drops_while_another_operation_holds_the_slot() {
    let ctx = egui::Context::default();
    for ok in [true, false] {
        let mut tab = held_by("Fold", &ctx);
        let (_, jobs) = crate::repo::messages::record_jobs(|| tab.busy_op_released(ok, &ctx));
        assert!(jobs.is_empty(), "nothing submitted (ok = {ok})");
        assert_eq!(tab.busy_op.as_deref(), Some("Fold"));
        assert_eq!(tab.waiting, [Waiter::Fetch]);
        assert_eq!(
            tab.status.as_deref().cloned(),
            Some(("Fold: preparing".to_owned(), false)),
            "no waiter named as not run (ok = {ok})"
        );
    }
}

/// Once the slot is free, a failure the status reports drops the waiters and names them there.
#[test]
fn a_reported_failure_drops_the_waiters_once_the_slot_is_free() {
    let ctx = egui::Context::default();
    let mut tab = held_by("Fold", &ctx);
    tab.busy_op = None;
    tab.status = Some(crate::repo::Notice::plain(("Fold: refused".into(), true)));
    let (_, jobs) = crate::repo::messages::record_jobs(|| tab.busy_op_released(false, &ctx));
    assert!(jobs.is_empty());
    assert!(tab.waiting.is_empty());
    let status = tab.status.as_deref().cloned().unwrap_or_default();
    assert!(status.1 && status.0.contains("Fetch"), "{status:?}");
}
