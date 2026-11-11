//! The constellation in a tab: a real repository read and drawn, and one that isn't there.

use std::time::{Duration, Instant};

use super::*;
use crate::git::fixture::Repo;
use crate::repo::insights::tests::draw;
use crate::repo::insights::{InsightsAct, Kind as View};

fn state(tab: &RepoTab) -> Option<&State> {
    tab.insights.as_ref().map(|v| &v.constellation)
}

#[test]
fn forge_retirement_rebuilds_equal_forge_data_while_retaining_local_snapshot() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = super::super::test_support::tab(&ctx, View::Constellation);
    let initial = &mut tab.insights.as_mut().unwrap().constellation;
    let request = initial.read.start();
    initial.read.accept(request, Ok(Snapshot::default()));
    tab.constellation_build();
    let fingerprint = tab.constellation_forge_read();
    tab.invalidate_forge_insights();
    let retired = state(&tab).unwrap();
    assert!(retired.read.ready().is_some());
    assert!(retired.built.is_some());
    assert!(retired.forge_read.is_none());
    assert_eq!(tab.constellation_forge_read(), fingerprint);
    draw(&mut tab, &ctx, 900.0);
    let state = state(&tab).unwrap();
    assert!(state.read.ready().is_some());
    assert_eq!(state.forge_read, Some(fingerprint));
}

/// Takes in background work until `done` holds, drawing the page each time.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let start = Instant::now();
    while !done(tab) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(ctx);
        draw(tab, ctx, 1200.0);
    }
}

#[test]
fn the_view_reads_a_real_repository_and_draws_both_ways() -> Result<(), String> {
    let repo = Repo::new("constellation-tab")?;
    repo.commit("base", &[("a.txt", Some("a\n"))])?;
    repo.git(&["branch", "feat"])?;
    let wt = repo.path().join("wt");
    repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), "feat"])?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(
        Action::Insights(InsightsAct::Open(View::Constellation)),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.built.is_some() && !s.disk.loading)
    });
    let built = state(&tab)
        .and_then(|s| s.built.as_ref())
        .ok_or("not built")?;
    let feat = built.model.find(Kind::Branch, "feat").ok_or("no feat")?;
    let main = built.model.find(Kind::Branch, "main").ok_or("no main")?;
    let worktree = built
        .model
        .nodes
        .iter()
        .position(|n| n.kind == Kind::Worktree && !n.detail.iter().any(|d| d.contains("main")))
        .ok_or("no worktree")?;
    assert!(
        built.graph.nodes[worktree].tip.contains("disk: "),
        "{}",
        built.graph.nodes[worktree].tip
    );
    // Without a forge, it says so rather than showing none.
    assert!(
        built
            .model
            .notes
            .iter()
            .any(|n| n.starts_with("Pull requests: not read"))
    );

    // Picked, then a second branch with it: the pair compares.
    tab.apply(
        act(Act::Select {
            node: worktree,
            add: false,
        }),
        &ctx,
    );
    draw(&mut tab, &ctx, 1200.0);
    tab.apply(
        act(Act::Select {
            node: main,
            add: false,
        }),
        &ctx,
    );
    tab.apply(
        act(Act::Select {
            node: feat,
            add: true,
        }),
        &ctx,
    );
    assert_eq!(state(&tab).and_then(|s| s.other), Some(feat));
    draw(&mut tab, &ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
    draw(&mut tab, &ctx, 600.0);
    // Esc takes back the second branch, then the first, then closes the page.
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert_eq!(
        state(&tab).map(|s| (s.selected, s.other)),
        Some((Some(main), None))
    );
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert_eq!(state(&tab).and_then(|s| s.selected), None);
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
    Ok(())
}

#[test]
fn a_repository_that_isnt_there_fails_with_a_retry() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/constellation".into(), &ctx);
    tab.apply(
        Action::Insights(InsightsAct::Open(View::Constellation)),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| !s.read.loading)
    });
    assert!(state(&tab).and_then(|s| s.read.error()).is_some());
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
}

/// A build's state arriving (or changing) after the constellation was drawn draws it again:
/// what the tab has of the forge reads differently.
#[test]
fn an_answer_from_the_forge_changes_what_it_was_drawn_with() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/constellation-forge".into(), &ctx);
    let before = tab.constellation_forge_read();
    assert_eq!(before, tab.constellation_forge_read());
    let status = |state| crate::ci::Status {
        state,
        checks: Vec::new(),
    };
    tab.ci
        .by_sha
        .insert("abc".to_owned(), status(crate::ci::CheckState::Running));
    let running = tab.constellation_forge_read();
    assert_ne!(before, running);
    tab.ci
        .by_sha
        .insert("abc".to_owned(), status(crate::ci::CheckState::Passed));
    assert_ne!(running, tab.constellation_forge_read());
}
