//! A forge operation that answers after the primary forge changed lets the tab's operation slot
//! go. What was asked for meanwhile and waits for it then starts, as after any operation (#195).

use super::*;
use crate::repo::running::Waiter;
use crate::repo::{Action, OpKind, Refresh};

/// Create PR runs (recorded, never executed); Fetch asked for meanwhile waits for it.
fn creating_with_fetch_waiting(ctx: &egui::Context) -> (RepoTab, JobSender, String) {
    let mut tab = tab(ctx);
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        tab.create_pr(
            "feat/x".into(),
            "main".into(),
            "Title".into(),
            String::new(),
            false,
            Vec::new(),
            ctx,
        );
    });
    assert_eq!(jobs.len(), 1, "the create is submitted, never run here");
    let label = tab.busy_op.clone().unwrap_or_default();
    assert!(!label.is_empty(), "Create PR holds the operation slot");
    // Its reply comes through the sender of the forge it started on.
    let reply = tab.forge_operation_sender();
    tab.apply(Action::Fetch, ctx);
    assert_eq!(tab.waiting, [Waiter::Fetch]);
    (tab, reply, label)
}

fn created(label: String) -> Msg {
    Msg::OpDone {
        label,
        result: Ok(String::new()),
        refresh: Refresh::All,
        kind: OpKind::plain(),
        quiet: false,
        stopped: 0,
    }
}

#[test]
fn stale_create_pr_completion_starts_the_fetch_queued_behind_it() {
    // Background network work stays paused; a Fetch the user asked for may still start
    // (Offline mode, `Pause::Switch`, would refuse it).
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let ctx = egui::Context::default();
    let mut other = tab(&ctx);
    let (mut tab, reply, label) = creating_with_fetch_waiting(&ctx);
    // Another tab of the same repository changes the primary forge; this one rebinds at the
    // start of its next poll, before it takes in the create's reply.
    other.set_primary_forge(Some("fork".into()), &ctx);
    let _ = reply.send(created(label));
    let (_, jobs) = crate::repo::messages::record_jobs(|| tab.poll(&ctx));
    assert_eq!(tab.hosting, Some(hosting("JKamsker")));
    assert_eq!(
        tab.busy_op.as_deref(),
        Some("Fetch"),
        "the queued Fetch starts once the stale create let the slot go: {:?} {:?}",
        tab.status.as_deref(),
        tab.waiting
    );
    assert!(tab.waiting.is_empty());
    assert!(
        !jobs.is_empty(),
        "the fetch is submitted (recorded, not run)"
    );
}

#[test]
fn stale_create_pr_after_a_remote_url_change_starts_the_queued_fetch_and_no_old_continuation() {
    // Background network work stays paused; a Fetch the user asked for may still start
    // (Offline mode, `Pause::Switch`, would refuse it).
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let ctx = egui::Context::default();
    let (mut tab, reply, label) = creating_with_fetch_waiting(&ctx);
    let read = tab.forge_sender();
    // A reload whose remotes changed rebinds the selection the same way.
    tab.remotes[0] = remote("origin", "changed");
    tab.forge_remotes[0].1 = hosting("changed");
    tab.take_forge_selection(true, &ctx);
    let _ = read.send(Msg::Then(Box::new(|tab, _| {
        tab.prs = vec![pr(7, "Vancete")];
    })));
    let _ = reply.send(created(label));
    crate::repo::messages::record_jobs(|| tab.poll(&ctx));
    assert!(tab.prs.is_empty(), "no continuation of the old forge ran");
    assert_eq!(tab.busy_op.as_deref(), Some("Fetch"));
    assert!(tab.waiting.is_empty());
    assert!(
        tab.running.is_some(),
        "the Fetch follows its own progress, not the stale create's"
    );
}
