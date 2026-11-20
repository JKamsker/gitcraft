//! Approving a fork's workflow runs ends outside `op_done`. Fetch, Pull and Push asked for while
//! it ran wait for it, and its answer has to start them, or drop them after a failure (#195).

use super::*;
use crate::repo::running::Waiter;

/// The approval holds the operation slot (as `approve_runs` leaves it) and Fetch waits for it.
fn approving_with_fetch_waiting(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/fork-approval-queue".into(), ctx);
    tab.busy_op = Some("Approve #7's workflow runs".into());
    tab.op_started = Some(Instant::now());
    tab.forks.approving = true;
    tab.apply(crate::repo::Action::Fetch, ctx);
    assert_eq!(tab.waiting, [Waiter::Fetch]);
    tab
}

#[test]
fn approval_answer_starts_the_fetch_queued_behind_it() {
    let ctx = egui::Context::default();
    let mut tab = approving_with_fetch_waiting(&ctx);
    let (_, jobs) =
        crate::repo::messages::record_jobs(|| tab.handle(approval_result(7, Ok(())), &ctx));
    assert!(!tab.forks.approving);
    assert_eq!(tab.busy_op.as_deref(), Some("Fetch"));
    assert!(tab.waiting.is_empty());
    assert!(
        !jobs.is_empty(),
        "the fetch is submitted (recorded, not run)"
    );
}

#[test]
fn failed_approval_drops_the_fetch_queued_behind_it_and_says_so() {
    let ctx = egui::Context::default();
    let mut tab = approving_with_fetch_waiting(&ctx);
    crate::repo::messages::record_jobs(|| {
        tab.handle(approval_result(7, Err("forbidden".into())), &ctx)
    });
    assert!(tab.busy_op.is_none());
    assert!(tab.op_started.is_none());
    assert!(tab.waiting.is_empty());
    let status = tab.status.as_deref().cloned().unwrap_or_default();
    assert!(status.1, "{status:?}");
    assert!(status.0.contains("forbidden"), "{status:?}");
    assert!(
        status.0.contains("Fetch"),
        "names what was not run: {status:?}"
    );
}
