//! The conflict radar in a tab: three branches of a real repository read, picked and drawn
//! both ways, read again after a tip moved; and a repository that isn't there.

use std::time::{Duration, Instant};

use super::*;
use crate::git::fixture::Repo;
use crate::repo::insights::tests::draw;
use crate::repo::insights::{InsightsAct, Kind};

fn state(tab: &RepoTab) -> Option<&State> {
    tab.insights.as_ref().map(|v| &v.conflict_radar)
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

fn radar(tab: &RepoTab) -> Option<&Radar> {
    state(tab).and_then(|s| s.read.ready())
}

#[test]
fn the_radar_reads_picks_and_reuses_pairs_in_a_tab() -> Result<(), String> {
    let repo = Repo::new("radar-tab")?;
    repo.commit("base", &[("f.txt", Some("1\n2\n3\n"))])?;
    for (branch, text) in [("a", "1\nA\n3\n"), ("b", "1\nB\n3\n")] {
        repo.git(&["checkout", "-q", "-b", branch, "main"])?;
        repo.commit(branch, &[("f.txt", Some(text))])?;
    }
    repo.git(&["checkout", "-q", "main"])?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(
        Action::Insights(InsightsAct::Open(Kind::ConflictRadar)),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| radar(t).is_some());
    let r = radar(&tab).ok_or("not read")?;
    assert_eq!((r.computed, r.reused), (3, 0));
    let pair = r
        .edges
        .iter()
        .position(|e| !e.result.conflicts().is_empty())
        .ok_or("no conflict")?;
    let layout = state(&tab)
        .and_then(|s| s.layout.as_ref())
        .ok_or("no picture")?;
    // The conflicting pair has a node of its own, a diamond.
    let node = layout
        .items
        .iter()
        .position(|i| *i == Item::Pair(pair))
        .ok_or("no pair node")?;
    assert_eq!(layout.graph.nodes[node].shape, graph::NodeShape::Diamond);

    tab.apply(act(Act::SelectPair(pair)), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| s.hunks.ready().is_some())
    });
    let hunks = state(&tab)
        .and_then(|s| s.hunks.ready())
        .ok_or("no hunks")?;
    assert_eq!(hunks.len(), 1);
    draw(&mut tab, &ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
    draw(&mut tab, &ctx, 600.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);

    // Read again: nothing moved, every pair is reused and the pick stays.
    tab.apply(Action::Insights(InsightsAct::Reload), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| !s.read.loading)
    });
    assert_eq!(radar(&tab).map(|r| (r.computed, r.reused)), Some((0, 3)));
    assert_eq!(
        state(&tab).and_then(|s| s.selected),
        Some(Picked::Pair(pair))
    );
    // `b` moves: its two pairs are merged again.
    repo.git(&["checkout", "-q", "b"])?;
    repo.commit("more", &[("g.txt", Some("g\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    tab.apply(Action::Insights(InsightsAct::Reload), &ctx);
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| !s.read.loading)
    });
    assert_eq!(radar(&tab).map(|r| (r.computed, r.reused)), Some((2, 1)));
    draw(&mut tab, &ctx, 1200.0);

    // Esc takes back the pick, then closes the page.
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
    let mut tab = RepoTab::new_deferred("/nonexistent/conflict-radar".into(), &ctx);
    tab.apply(
        Action::Insights(InsightsAct::Open(Kind::ConflictRadar)),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        state(t).is_some_and(|s| !s.read.loading)
    });
    assert!(state(&tab).and_then(|s| s.read.error()).is_some());
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
}

#[test]
fn pair_nodes_keep_clear_of_each_other() {
    // Six branches around the default one: the long chords all cross the middle.
    let pairs: Vec<(usize, usize)> = (0..6)
        .flat_map(|i| (i + 1..7).map(move |j| (i, j)))
        .collect();
    let pos = picture::place(6, true, &pairs);
    assert_eq!(pos.len(), 7 + pairs.len());
    assert!(
        pos.iter()
            .all(|p| (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y))
    );
    // The default branch is in the middle, and the pairs with it aren't on top of it.
    assert_eq!(pos[6], egui::pos2(0.5, 0.5));
    for p in &pos[7..] {
        assert!((*p - pos[6]).length() > 0.01);
    }
}

/// Two branches and the default one: not on one line, so no pair sits on another node.
#[test]
fn two_branches_and_the_default_one_make_a_triangle() {
    let pairs = [(0, 1), (0, 2), (1, 2)];
    let pos = picture::place(2, true, &pairs);
    let (a, b, t) = (pos[0], pos[1], pos[2]);
    let cross = (b - a).x * (t - a).y - (b - a).y * (t - a).x;
    assert!(cross.abs() > 0.05, "not on one line: {pos:?}");
    for (i, p) in pos.iter().enumerate().skip(3) {
        for q in &pos[..3] {
            assert!((*p - *q).length() > 0.05, "pair {i} on a branch: {pos:?}");
        }
    }
}
