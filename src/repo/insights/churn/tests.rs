//! The timeline in a tab: without a repository, and over a real one.

use std::time::{Duration, Instant};

use super::super::tests::draw;
use super::super::{InsightsAct, Kind};
use super::{Act, State};
use crate::git;
use crate::repo::{Action, RepoTab};

fn state(tab: &RepoTab) -> Option<&State> {
    tab.insights.as_deref().map(|v| &v.churn)
}

/// Takes in background work until `done` holds, drawing the page each time.
fn wait(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let start = Instant::now();
    while !done(tab) {
        if start.elapsed() > Duration::from_secs(20) {
            return Err("timed out waiting for the timeline".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(ctx);
        draw(tab, ctx, 1200.0);
    }
    Ok(())
}

fn built(tab: &RepoTab) -> bool {
    state(tab).is_some_and(|s| s.built.ready().is_some() && !s.built.loading && !s.read.loading)
}

fn both_ways(tab: &mut RepoTab, ctx: &egui::Context) {
    draw(tab, ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
    draw(tab, ctx, 1200.0);
    draw(tab, ctx, 600.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
}

#[test]
fn without_a_repository_the_timeline_says_it_could_not_read() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-churn".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Churn)), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.read.error().is_some())
    })?;
    both_ways(&mut tab, &ctx);
    assert!(!tab.churn_select(0, 0));
    assert!(!tab.churn_back());
    Ok(())
}

#[test]
fn over_a_real_repository_points_select_their_windows() -> Result<(), String> {
    let repo = git::fixture::Repo::new("insights-churn-tab")?;
    repo.commit("one", &[("src/lib.rs", Some("fn a() {}\n"))])?;
    repo.commit(
        "two",
        &[
            ("src/lib.rs", Some("fn a() { if x {} }\n")),
            ("notes.md", Some("n\n")),
        ],
    )?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Churn)), &ctx);
    wait(&mut tab, &ctx, built)?;
    both_ways(&mut tab, &ctx);
    let b = state(&tab)
        .and_then(|s| s.built.ready())
        .ok_or("built")?
        .clone();
    assert_eq!(b.timeline.samples.len(), 2);
    assert_eq!(b.timeline.rows[0].name, "src/lib.rs");

    // A point selected lists its window; drawn both ways; Esc takes it back.
    assert!(tab.churn_select(0, 1));
    assert!(!tab.churn_select(0, 9));
    both_ways(&mut tab, &ctx);
    assert_eq!(state(&tab).and_then(|s| s.selected), Some((0, 1)));
    assert!(tab.churn_back());
    assert!(!tab.churn_back());

    // Folders.
    if let Some(v) = tab.insights.as_deref_mut() {
        v.churn.options.group = Some(1);
    }
    tab.apply(Action::Insights(InsightsAct::Churn(Act::Rebuild)), &ctx);
    wait(&mut tab, &ctx, |t| {
        built(t)
            && state(t)
                .and_then(|s| s.built.ready())
                .is_some_and(|b| b.timeline.rows.iter().all(|r| r.folder))
    })?;
    both_ways(&mut tab, &ctx);

    // A new commit, read again: the last sample is the new tip.
    let third = repo.commit("three", &[("src/lib.rs", Some("fn a() {}\n"))])?;
    tab.apply(Action::Insights(InsightsAct::Reload), &ctx);
    wait(&mut tab, &ctx, |t| {
        built(t)
            && state(t)
                .and_then(|s| s.built.ready())
                .is_some_and(|b| b.history.tip == third)
    })?;
    let b = state(&tab).and_then(|s| s.built.ready()).ok_or("built")?;
    assert_eq!(
        b.timeline.samples.last().map(|s| s.sample.hash.as_str()),
        Some(third.as_str())
    );
    Ok(())
}
