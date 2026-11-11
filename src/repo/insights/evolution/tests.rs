//! The heatmap in a tab: without a repository, and over a real one.

use std::time::{Duration, Instant};

use super::super::tests::draw;
use super::super::{InsightsAct, Kind};
use super::{Act, Pick, State};
use crate::git;
use crate::repo::{Action, DiffKey, RepoTab};

fn state(tab: &RepoTab) -> Option<&State> {
    tab.insights.as_deref().map(|v| &v.evolution)
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
            return Err("timed out waiting for the heatmap".to_owned());
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
fn without_a_repository_the_heatmap_says_it_could_not_read() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-evolution".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Evolution)), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.read.error().is_some())
    })?;
    both_ways(&mut tab, &ctx);
    assert!(!tab.evolution_back());
    Ok(())
}

#[test]
fn over_a_real_repository_cells_open_diffs_and_commits() -> Result<(), String> {
    let repo = git::fixture::Repo::new("insights-evolution-tab")?;
    let first = repo.commit("one", &[("src/a.rs", Some("a\n")), ("b.txt", Some("b\n"))])?;
    repo.commit(
        "two",
        &[("src/a.rs", Some("a\nb\n")), ("src/c.rs", Some("c\n"))],
    )?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Evolution)), &ctx);
    wait(&mut tab, &ctx, built)?;
    both_ways(&mut tab, &ctx);

    let b = state(&tab)
        .and_then(|s| s.built.ready())
        .ok_or("built")?
        .clone();
    assert_eq!(b.matrix.columns.len(), 2);
    let a = b
        .matrix
        .rows
        .iter()
        .position(|r| r.name == "src/a.rs")
        .ok_or("a row for src/a.rs")?;
    let Some(Action::OpenDiff(DiffKey::Commit { hash, path })) = tab.evolution_open(a, 0) else {
        return Err("a file's cell opens its diff".to_owned());
    };
    assert_eq!((hash, path.as_str()), (first, "src/a.rs"));
    assert_eq!(state(&tab).and_then(|s| s.selected), Some((a, 0)));

    // Folders: a folder's cell lists its commits.
    if let Some(v) = tab.insights.as_deref_mut() {
        v.evolution.options.group = Some(1);
    }
    tab.apply(Action::Insights(InsightsAct::Evolution(Act::Rebuild)), &ctx);
    wait(&mut tab, &ctx, |t| {
        built(t)
            && state(t)
                .and_then(|s| s.built.ready())
                .is_some_and(|b| b.matrix.rows.iter().all(|r| r.folder))
    })?;
    assert!(tab.evolution_open(0, 1).is_none());
    assert_eq!(
        state(&tab).and_then(|s| s.pick.clone()),
        Some(Pick::Cell(0, 1))
    );
    both_ways(&mut tab, &ctx);
    assert!(tab.evolution_back());
    assert!(tab.evolution_back());
    assert!(!tab.evolution_back());

    // A new commit, read again: the matrix follows the new tip.
    let third = repo.commit("three", &[("src/c.rs", Some("c\nd\n"))])?;
    tab.apply(Action::Insights(InsightsAct::Reload), &ctx);
    wait(&mut tab, &ctx, |t| {
        built(t)
            && state(t)
                .and_then(|s| s.built.ready())
                .is_some_and(|b| b.history.tip == third)
    })?;
    let b = state(&tab).and_then(|s| s.built.ready()).ok_or("built")?;
    assert_eq!(b.matrix.columns.len(), 3);
    assert_eq!(b.provenance.tips[0].1, third);
    Ok(())
}
