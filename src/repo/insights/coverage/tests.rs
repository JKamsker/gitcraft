//! The coverage map in a tab: without a repository to read, and on a real one with a matching
//! report, drawn as a picture and as tables, its hunks opening at the revision.

use std::time::{Duration, Instant};

use super::super::tests::draw;
use super::super::{InsightsAct, Kind};
use super::{Act, NodeRef, open_node};
use crate::insights::Evidence;
use crate::insights::coverage::Ask;
use crate::repo::insights::graph::Pick;
use crate::repo::{Action, RepoTab};

impl crate::repo::insights::context::namespace_tests::Fixture for super::State {
    fn seed(&mut self) {
        use crate::repo::insights::context::namespace_tests::seed_slot;
        seed_slot(&mut self.data);
        seed_slot(&mut self.layout);
        self.report = Some("keep.lcov".into());
        self.typed = "keep.lcov".into();
        self.test = Some(7);
    }
    fn assert_retired(&self) {
        use crate::repo::insights::context::namespace_tests::assert_slot_retired;
        assert_slot_retired(&self.data);
        assert_slot_retired(&self.layout);
        assert!(self.selected.is_none() && self.test.is_none());
        assert_eq!(self.report.as_deref(), Some("keep.lcov"));
        assert_eq!(self.typed, "keep.lcov");
    }
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let start = Instant::now();
    while !done(tab) {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(ctx);
        draw(tab, ctx, 1200.0);
    }
}

fn both_ways(tab: &mut RepoTab, ctx: &egui::Context) {
    draw(tab, ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
    draw(tab, ctx, 1200.0);
    draw(tab, ctx, 600.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
    draw(tab, ctx, 1200.0);
}

fn settled(tab: &RepoTab) -> bool {
    tab.insights.as_ref().is_some_and(|v| {
        v.coverage.data.value.is_some()
            && !v.coverage.data.loading
            && (v.coverage.data.error().is_some() || v.coverage.layout.ready().is_some())
            && !v.coverage.layout.loading
    })
}

#[test]
fn without_a_repository_the_map_says_it_could_not_read_it() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-coverage".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Coverage)), &ctx);
    wait(&mut tab, &ctx, settled);
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.coverage.data.error().is_some())
    );
    both_ways(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
}

#[test]
fn a_commit_with_a_matching_report_is_drawn_and_opens_its_hunks() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("coverage-tab")?;
    repo.commit(
        "one",
        &[
            ("src/lib.rs", Some("fn a() {}\n")),
            (".gitignore", Some("lcov.info*\n")),
        ],
    )?;
    let two = repo.commit("two", &[("src/lib.rs", Some("fn a() {\n    1\n}\n"))])?;
    std::fs::write(
        repo.path().join("lcov.info"),
        "TN:t1\nSF:src/lib.rs\nDA:1,1\nDA:2,0\nend_of_record\nTN:t2\nSF:src/lib.rs\nDA:1,1\nend_of_record\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("lcov.info.sha"), &two).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    tab.detail_hash = Some(two.clone());
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Coverage)), &ctx);
    wait(&mut tab, &ctx, settled);
    // The working tree is clean: nothing changed there.
    let clean = tab
        .insights
        .as_ref()
        .and_then(|v| v.coverage.data.ready().cloned())
        .ok_or("not read")?;
    assert!(clean.files.is_empty());
    both_ways(&mut tab, &ctx);

    tab.apply(
        Action::Insights(InsightsAct::Coverage(Act::Ask(Ask::Commit(two.clone())))),
        &ctx,
    );
    wait(&mut tab, &ctx, settled);
    let view = tab.insights.as_ref().ok_or("closed")?;
    let data = view.coverage.data.ready().ok_or("not read")?.clone();
    let laid = view.coverage.layout.ready().ok_or("not laid out")?.clone();
    assert_eq!(
        data.report.as_ref().map(|r| r.freshness.evidence),
        Some(Evidence::Observed)
    );
    // The file, its hunk and the two tests that ran it.
    assert_eq!(laid.nodes.len(), 4, "{:?}", laid.nodes);
    let hunk = laid
        .nodes
        .iter()
        .find(|n| matches!(n, NodeRef::Hunk(..)))
        .copied()
        .ok_or("no hunk")?;
    match open_node(&data, hunk, None).first() {
        Some(Action::OpenDiffOfLines { hash, path, lines }) => {
            assert_eq!(
                (hash.as_str(), path.as_str(), lines.as_slice()),
                (two.as_str(), "src/lib.rs", &[(1, 3)][..])
            );
        }
        _ => return Err("the hunk opened something else".to_owned()),
    }
    match open_node(&data, NodeRef::File(0), None).first() {
        Some(Action::FileView { rev, .. }) => assert_eq!(rev.as_deref(), Some(two.as_str())),
        _ => return Err("the file opened something else".to_owned()),
    }
    tab.apply(
        Action::Insights(InsightsAct::Coverage(Act::Select(Some(Pick::Node(1))))),
        &ctx,
    );
    tab.apply(
        Action::Insights(InsightsAct::Coverage(Act::Test(Some(0)))),
        &ctx,
    );
    wait(&mut tab, &ctx, settled);
    both_ways(&mut tab, &ctx);
    // Esc takes back the selection, then the test, then closes the page.
    for open in [true, true, false] {
        tab.apply(Action::Insights(InsightsAct::Back), &ctx);
        assert_eq!(tab.insights.is_some(), open);
    }
    Ok(())
}
