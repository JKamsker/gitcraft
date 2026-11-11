//! The repeated-edit view in a tab: reading a repository's commit, drawing its matrix and
//! table, opening a cell, and without a repository.

use std::time::{Duration, Instant};

use super::*;
use crate::repo::insights::{InsightsAct, Kind, tests::draw};

impl crate::repo::insights::context::namespace_tests::Fixture for State {
    fn seed(&mut self) {
        crate::repo::insights::context::namespace_tests::seed_slot(&mut self.read);
        self.of = Of::Branch;
        self.hide_once = true;
    }
    fn assert_retired(&self) {
        crate::repo::insights::context::namespace_tests::assert_slot_retired(&self.read);
        assert!(self.asked.is_none() && self.selected.is_none());
        assert_eq!(self.of, Of::Branch);
        assert!(self.hide_once);
    }
}

#[test]
fn forge_retirement_clears_only_pr_owned_reads_and_preserves_display_options() {
    for of in [Of::Commit, Of::Branch, Of::Pr(7)] {
        let mut state = State {
            of,
            asked: Some((of, None)),
            hide_once: true,
            ..Default::default()
        };
        let old = state.read.start();
        let pr_owned = matches!(of, Of::Pr(_));
        assert_eq!(state.invalidate_forge(), pr_owned);
        assert_eq!(
            state.read.accept(old, Err("old PR reply".into())),
            !pr_owned
        );
        assert!(state.hide_once);
        assert_eq!(state.of, if pr_owned { Of::Commit } else { of });
        assert_eq!(state.read.value.is_none(), pr_owned);
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

fn read(tab: &RepoTab) -> Option<&Transform> {
    tab.insights.as_ref()?.transform.read.ready()
}

fn answered(tab: &RepoTab) -> bool {
    tab.insights
        .as_ref()
        .is_some_and(|v| v.transform.read.value.is_some() && !v.transform.read.loading)
}

#[test]
fn the_view_reads_a_commit_and_draws_its_matrix_and_table() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("insights-transform-view")?;
    let files = ["a.rs", "b.rs", "c.rs"];
    let old: Vec<(&str, Option<&str>)> = files.iter().map(|f| (*f, Some("foo(x);\n"))).collect();
    repo.commit("one", &old)?;
    let new: Vec<(&str, Option<&str>)> =
        files.iter().map(|f| (*f, Some("foo(x, ctx);\n"))).collect();
    let head = repo.commit("two", &new)?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Transform)), &ctx);
    wait(&mut tab, &ctx, answered);
    let t = read(&tab).ok_or("not read")?;
    assert_eq!(t.subject.commit.as_deref(), Some(head.as_str()));
    assert_eq!(t.patterns.len(), 1);
    // The same name everywhere: not a placeholder.
    assert_eq!(t.patterns[0].label(), "foo(x); → foo(x, ctx);");
    // A cell opens the commit's diff at the edit's lines.
    let opened = open(t, &[0]);
    let at = match &opened {
        Some(Action::OpenDiffOfLines { hash, path, lines }) => {
            Some((hash.clone(), path.clone(), lines.clone()))
        }
        _ => None,
    };
    assert_eq!(at, Some((head.clone(), "a.rs".to_owned(), vec![(1, 1)])));
    tab.apply(act(Act::Select(Some((0, 1)))), &ctx);
    draw(&mut tab, &ctx, 1200.0);
    tab.apply(act(Act::HideOnce(true)), &ctx);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
    draw(&mut tab, &ctx, 600.0);
    // Esc takes back the cell first.
    assert!(tab.transform_back());
    assert!(!tab.transform_back());
    // With no default branch known (the tab read no branches), there is no range: said so.
    tab.apply(act(Act::Of(Of::Branch)), &ctx);
    wait(&mut tab, &ctx, answered);
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.transform.read.error().is_some())
    );
    draw(&mut tab, &ctx, 1200.0);
    Ok(())
}

#[test]
fn without_a_repository_the_view_says_it_couldnt_read() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-transform".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Transform)), &ctx);
    wait(&mut tab, &ctx, answered);
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.transform.read.error().is_some())
    );
    // A pull request that isn't listed is said to be unknown.
    tab.apply(act(Act::Of(Of::Pr(9))), &ctx);
    wait(&mut tab, &ctx, answered);
    let error = tab
        .insights
        .as_ref()
        .and_then(|v| v.transform.read.error().map(str::to_owned))
        .unwrap_or_default();
    assert!(error.contains("#9"), "{error}");
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx, 1200.0);
}
