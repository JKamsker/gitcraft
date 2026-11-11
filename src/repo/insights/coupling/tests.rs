//! The change-coupling view in a tab: reading a repository, drawing it, and without one.

use super::*;
use crate::repo::insights::{InsightsAct, Kind, tests::draw};
use std::time::{Duration, Instant};

impl crate::repo::insights::context::namespace_tests::Fixture for State {
    fn seed(&mut self) {
        use crate::repo::insights::context::namespace_tests::seed_slot;
        seed_slot(&mut self.read);
        seed_slot(&mut self.commit_files);
        self.filter = "keep-folder".into();
        self.limit = 42;
        self.range = 1;
        self.pair = Some(7);
    }
    fn assert_retired(&self) {
        use crate::repo::insights::context::namespace_tests::assert_slot_retired;
        assert_slot_retired(&self.read);
        assert_slot_retired(&self.commit_files);
        assert!(self.pair.is_none() && self.unit.is_none() && self.left.is_none());
        assert_eq!(self.filter, "keep-folder");
        assert_eq!((self.limit, self.range), (42, 1));
    }
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let start = Instant::now();
    while !done(tab) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(ctx);
        draw(tab, ctx, 1200.0);
    }
}

fn model(tab: &RepoTab) -> Option<&Model> {
    tab.insights.as_ref()?.coupling.read.ready()
}

#[test]
fn the_view_reads_a_repository_and_draws_its_network_and_tables() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("insights-coupling-view")?;
    for n in 0..3 {
        let text = format!("{n}\n");
        repo.commit(
            "both",
            &[("src/a.rs", Some(&text)), ("src/b.rs", Some(&text))],
        )?;
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Coupling)), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.insights
            .as_ref()
            .is_some_and(|v| v.coupling.read.value.is_some())
    });
    let m = model(&tab).ok_or("not read")?;
    assert_eq!(m.coupling.pairs.len(), 1);
    assert_eq!(m.positions.len(), 2);
    tab.apply(act(Act::Pair(Some(0))), &ctx);
    tab.apply(act(Act::Change(Change::Commit)), &ctx);
    draw(&mut tab, &ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
    draw(&mut tab, &ctx, 600.0);
    // Counted again as folders: one folder, no pairs.
    tab.apply(
        act(Act::Options(Options {
            depth: Some(1),
            ..Options::default()
        })),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        model(t).is_some_and(|m| m.coupling.options.depth == Some(1))
    });
    assert!(model(&tab).is_some_and(|m| m.coupling.pairs.is_empty()));
    draw(&mut tab, &ctx, 1200.0);
    // Esc takes back the change looked at, then closes the page.
    assert!(tab.coupling_back());
    assert!(!tab.coupling_back());
    Ok(())
}

#[test]
fn without_a_repository_the_view_says_it_couldnt_read() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-coupling".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Coupling)), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.insights
            .as_ref()
            .is_some_and(|v| v.coupling.read.value.is_some())
    });
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.coupling.read.error().is_some())
    );
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
}
