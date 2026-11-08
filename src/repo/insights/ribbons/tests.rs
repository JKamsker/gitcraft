//! The ribbon map in a tab: with nothing to read, and on a repository with a rewrite kept.

use std::time::{Duration, Instant};

use super::{Act, Pick};
use crate::insights::ribbons::{Class, tests::rewritten_repo};
use crate::repo::insights::tests::draw;
use crate::repo::insights::{InsightsAct, Kind};
use crate::repo::{Action, RepoTab};

fn state(tab: &RepoTab) -> Option<&super::State> {
    tab.insights.as_ref().map(|v| &v.ribbons)
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
            return Err("timed out waiting for the ribbons".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(ctx);
        draw(tab, ctx, 1200.0);
    }
    Ok(())
}

fn both_ways(tab: &mut RepoTab, ctx: &egui::Context) {
    draw(tab, ctx, 1200.0);
    draw(tab, ctx, 600.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
    draw(tab, ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
}

#[test]
fn without_a_repository_there_is_nothing_to_compare() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-ribbons".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Ribbons)), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.rewrites.value.is_some())
    })?;
    assert!(state(&tab).is_some_and(|s| s.source.is_none()));
    both_ways(&mut tab, &ctx);
    tab.apply(
        Action::Insights(InsightsAct::Ribbons(Act::ToggleStill)),
        &ctx,
    );
    both_ways(&mut tab, &ctx);
    Ok(())
}

#[test]
fn a_kept_rewrite_is_drawn_and_its_ribbons_open_their_patches() -> Result<(), String> {
    let (repo, old, new) = rewritten_repo("ribbons-tab")?;
    crate::git::record_rewrite(repo.path(), "rewritten", &old, &new, 1_700_000_000)?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Ribbons)), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.data.value.is_some())
    })?;
    let data = state(&tab)
        .and_then(|s| s.data.ready())
        .ok_or("the comparison failed")?;
    assert_eq!(data.provenance.stale, None);
    let squash = data
        .ribbons
        .iter()
        .position(|r| r.class == Class::Squashed && r.pair.is_none())
        .ok_or("no inferred squash")?;
    both_ways(&mut tab, &ctx);
    tab.apply(
        Action::Insights(InsightsAct::Ribbons(Act::Select(Pick::Ribbon(squash)))),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.patch.value.is_some())
    })?;
    assert!(state(&tab).is_some_and(|s| s.patch.ready().is_some()));
    both_ways(&mut tab, &ctx);
    // A dropped commit shows its own patch.
    tab.apply(
        Action::Insights(InsightsAct::Ribbons(Act::Select(Pick::Old(2)))),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.patch.ready().is_some())
    })?;
    // Esc lets go of the selection first, then closes the page.
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(state(&tab).is_some_and(|s| s.selected.is_none()));
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
    Ok(())
}
