use super::*;

#[test]
fn cancelled_closed_or_superseded_activity_cannot_accept_old_answers() {
    let mut view = Activity::new(Page::Calendar);
    let generation = view.generation.fetch_add(1, Ordering::Relaxed) + 1;
    view.loading = true;
    view.cancel();
    view.accept(
        generation,
        Ok((Arc::new(History::default()), Summary::default())),
    );
    assert!(view.history.is_none());
    assert!(!view.loading);
    let lifetime = Arc::downgrade(&view.generation);
    drop(view);
    assert!(lifetime.upgrade().is_none());
}

#[test]
fn activity_range_validates_input_and_normalizes_backward_drag() -> Result<(), String> {
    let mut view = Activity::new(Page::Calendar);
    let a = Day::parse("2026-02-15").ok_or("day")?;
    let b = Day::parse("2026-01-01").ok_or("day")?;
    view.range(a, b);
    let query = view.requested_range()?;
    assert_eq!(query.from, Some(b));
    assert_eq!(query.through, Some(a));
    view.from = "bad".into();
    assert!(view.requested_range().is_err());
    view.from = "2027-01-01".into();
    assert!(view.requested_range().is_err());
    Ok(())
}

#[test]
fn reopening_the_same_repository_cannot_receive_a_closed_views_answer() {
    let mut old = Activity::new(Page::Calendar);
    let request = old.begin();
    drop(old);
    let mut reopened = Activity::new(Page::Calendar);
    let current = reopened.begin();
    assert_ne!(request, current);
    reopened.accept(
        request,
        Ok((Arc::new(History::default()), Summary::default())),
    );
    assert!(reopened.history.is_none());
    assert!(reopened.loading);
}

#[test]
fn calendar_and_contributors_buttons_change_the_actual_view_and_select_commit() -> Result<(), String>
{
    use crate::git::activity::Record;
    use egui_kittest::{Harness, kittest::Queryable};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/activity-ui".into(), &ctx);
    // Keep the synthetic tab deferred. Only the supplied history's filtering worker is
    // polled below; repository discovery and other ambient readers have no fixture data.
    let first = Day::parse("2026-01-01").ok_or("date")?;
    let last = Day::parse("2026-01-03").ok_or("date")?;
    let records = [(first, "first", "a"), (last, "later", "b")]
        .into_iter()
        .map(|(date, subject, hash)| Record {
            hash: hash.repeat(40),
            name: "A".into(),
            email: "a@test.invalid".into(),
            author: date,
            committer: last,
            subject: subject.into(),
            paths: Default::default(),
        })
        .collect();
    let history = Arc::new(History {
        commits: records,
        ..Default::default()
    });
    let summary = activity::summarize(&history, Query::default(), &|| true)?;
    let mut view = Activity::new(Page::Calendar);
    view.accept(0, Ok((history.clone(), summary)));
    tab.history_activity = Some(view);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 1100.0))
        .build_ui_state(
            |ui, state: &mut (RepoTab, bool)| {
                if !state.1 {
                    crate::theme::install_fonts(ui.ctx());
                    state.1 = true;
                }
                state.0.poll_history_activity(ui.ctx());
                let mut acts = Vec::new();
                state.0.activity_view(ui, &mut acts);
                for action in acts {
                    // Exercise real commit selection, but cancel its detail I/O before
                    // submission: these synthetic hashes have no repository objects.
                    let reads = state.0.tx.cancel.clone();
                    if matches!(&action, crate::repo::Action::ActivityCommit(_)) {
                        state.0.tx.cancel = reads.child();
                        state.0.tx.cancel.cancel();
                    }
                    state.0.apply(action, ui.ctx());
                    state.0.tx.cancel = reads;
                }
            },
            (tab, false),
        );
    harness.run();
    harness.get_by_label("1\n1 commits").click();
    harness.run_steps(2);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while harness
        .state()
        .0
        .history_activity
        .as_ref()
        .is_some_and(|v| v.loading)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.run_steps(1);
    }
    let settled = harness.state().0.history_activity.as_ref().ok_or("view")?;
    assert!(!settled.loading, "activity filter did not finish");
    assert!(settled.error.is_none(), "{:?}", settled.error);
    assert!(Arc::ptr_eq(
        settled.history.as_ref().ok_or("history")?,
        &history
    ));
    let view = harness.state().0.history_activity.as_ref().ok_or("view")?;
    assert_eq!(view.query.from, Some(first));
    assert_eq!(view.query.through, Some(first));
    assert_eq!(view.summary.as_ref().ok_or("summary")?.selected.len(), 1);
    harness
        .get_by_label("2026-01-01  aaaaaaaa  first · A")
        .click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().0.detail_hash.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    let start = harness.get_by_label("1\n1 commits").rect().center();
    let end = harness.get_by_label("3\n1 commits").rect().center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run_steps(1);
    harness.hover_at(end);
    harness.run_steps(2);
    harness.drop_at(end);
    harness.run_steps(2);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while harness
        .state()
        .0
        .history_activity
        .as_ref()
        .is_some_and(|v| v.loading)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.run_steps(1);
    }
    let settled = harness.state().0.history_activity.as_ref().ok_or("view")?;
    assert!(!settled.loading, "activity filter did not finish");
    assert!(settled.error.is_none(), "{:?}", settled.error);
    assert!(Arc::ptr_eq(
        settled.history.as_ref().ok_or("history")?,
        &history
    ));
    let view = harness
        .state()
        .0
        .history_activity
        .as_ref()
        .ok_or("drag view")?;
    assert_eq!(view.query.from, Some(first));
    assert_eq!(view.query.through, Some(last));
    harness.get_by_label("Browse months").click();
    harness.run();
    harness.get_by_label("Previous year").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .0
            .history_activity
            .as_ref()
            .ok_or("year view")?
            .month,
        Day::parse("2025-01-01")
    );
    harness.get_by_label("Next year").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .0
            .history_activity
            .as_ref()
            .ok_or("year view")?
            .month,
        Day::parse("2026-01-01")
    );
    harness.get_by_label("2026-01\n2 commits").click();
    harness.run_steps(2);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while harness
        .state()
        .0
        .history_activity
        .as_ref()
        .is_some_and(|v| v.loading)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.run_steps(1);
    }
    let settled = harness.state().0.history_activity.as_ref().ok_or("view")?;
    assert!(!settled.loading, "activity filter did not finish");
    assert!(settled.error.is_none(), "{:?}", settled.error);
    assert!(Arc::ptr_eq(
        settled.history.as_ref().ok_or("history")?,
        &history
    ));
    assert_eq!(
        harness
            .state()
            .0
            .history_activity
            .as_ref()
            .ok_or("month view")?
            .query
            .through,
        Day::parse("2026-01-31")
    );
    harness.get_by_label("Committer date").click();
    harness.run_steps(2);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while harness
        .state()
        .0
        .history_activity
        .as_ref()
        .is_some_and(|v| v.loading)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.run_steps(1);
    }
    let settled = harness.state().0.history_activity.as_ref().ok_or("view")?;
    assert!(!settled.loading, "activity filter did not finish");
    assert!(settled.error.is_none(), "{:?}", settled.error);
    assert!(Arc::ptr_eq(
        settled.history.as_ref().ok_or("history")?,
        &history
    ));
    assert_eq!(
        harness
            .state()
            .0
            .history_activity
            .as_ref()
            .ok_or("clock view")?
            .query
            .clock,
        Clock::Committer
    );
    harness.get_by_label("Contributor statistics").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .0
            .history_activity
            .as_ref()
            .ok_or("view")?
            .page,
        Page::Contributors
    );
    harness.get_by_label("A <a@test.invalid>").click();
    harness.run();
    assert!(
        harness
            .state()
            .0
            .history_activity
            .as_ref()
            .ok_or("view")?
            .author
            .is_some()
    );
    Ok(())
}
