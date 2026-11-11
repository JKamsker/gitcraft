//! The impact graph in a tab: without a repository to read, and on a real one, drawn as a
//! picture and as tables, its nodes opening at the revision they were read at.

use std::time::{Duration, Instant};

use super::super::tests::draw;
use super::super::{InsightsAct, Kind};
use super::{Act, NodeRef, open_node};
use crate::repo::insights::graph::Pick;
use crate::repo::{Action, RepoTab};

/// Takes in background work until `done` holds, drawing the page each time.
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

#[test]
fn without_a_repository_the_view_says_it_could_not_read_it() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-impact".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Impact)), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.insights
            .as_ref()
            .is_some_and(|v| v.impact.data.value.is_some())
    });
    let state = &tab
        .insights
        .as_ref()
        .map(|v| v.impact.data.error().is_some());
    assert_eq!(*state, Some(true));
    both_ways(&mut tab, &ctx);
    // Esc with nothing selected closes the page.
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
}

#[test]
fn a_real_commit_is_drawn_and_its_nodes_open_at_its_revision() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("impact-tab")?;
    repo.commit(
        "one",
        &[
            ("src/lib.rs", Some("pub fn compute() -> u32 {\n    1\n}\n")),
            (
                "src/user.rs",
                Some("fn run() -> u32 {\n    crate::compute()\n}\n"),
            ),
            (
                "tests/it.rs",
                Some("#[test]\nfn t() {\n    app::compute();\n}\n"),
            ),
        ],
    )?;
    let two = repo.commit(
        "two",
        &[
            ("src/lib.rs", Some("pub fn compute() -> u32 {\n    2\n}\n")),
            ("web/a.ts", Some("export {}\n")),
        ],
    )?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    tab.detail_hash = Some(two.clone());
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Impact)), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.insights
            .as_ref()
            .is_some_and(|v| v.impact.data.ready().is_some() && v.impact.layout.ready().is_some())
    });
    let view = tab.insights.as_ref().ok_or("closed")?;
    let data = view.impact.data.ready().ok_or("not read")?.clone();
    let laid = view.impact.layout.ready().ok_or("not laid out")?.clone();
    assert_eq!(data.range.head, two);
    // The symbol, the file no analyzer reads, the code and the test referring to it.
    assert_eq!(laid.graph.nodes.len(), 4, "{:?}", laid.nodes);
    assert_eq!(laid.graph.edges.len(), 2);
    let symbol = laid
        .nodes
        .iter()
        .position(|n| *n == NodeRef::Symbol(0))
        .ok_or("no symbol node")?;
    match open_node(&data, laid.nodes[symbol]) {
        Some(Action::BlameAt { path, rev, line }) => {
            assert_eq!(
                (path.as_str(), rev.as_deref(), line),
                ("src/lib.rs", Some(two.as_str()), 1)
            );
        }
        _ => return Err("the symbol opened something else".to_owned()),
    }
    for node in &laid.nodes {
        match open_node(&data, *node) {
            Some(Action::BlameAt { rev, .. }) => assert_eq!(rev.as_deref(), Some(two.as_str())),
            Some(Action::OpenDiff(_)) => assert!(matches!(node, NodeRef::Other(_))),
            _ => return Err(format!("{node:?} opened something else")),
        }
    }
    // An edge selected lists its lines; Esc takes the selection back first.
    tab.apply(
        Action::Insights(InsightsAct::Impact(Act::Select(Some(Pick::Edge(0))))),
        &ctx,
    );
    both_ways(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_some());
    // Filters lay the graph out again.
    tab.apply(
        Action::Insights(InsightsAct::Impact(Act::Filter(super::Filter {
            scope: super::Scope::InDiff,
            ..Default::default()
        }))),
        &ctx,
    );
    tab.apply(
        Action::Insights(InsightsAct::Impact(Act::Force(true))),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        t.insights
            .as_ref()
            .is_some_and(|v| !v.impact.layout.loading)
    });
    let laid = tab
        .insights
        .as_ref()
        .and_then(|v| v.impact.layout.ready().cloned())
        .ok_or("not laid out")?;
    assert!(laid.graph.edges.is_empty());
    both_ways(&mut tab, &ctx);
    Ok(())
}

/// Opened over a pull request's page, the graph is first about that pull request.
#[test]
fn the_pull_request_the_page_was_opened_over_is_the_default_change() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-impact-pr".into(), &ctx);
    tab.prs = vec![crate::forge::PullRequest {
        number: 7,
        title: "Seven".to_owned(),
        branch: "topic".to_owned(),
        base: "main".to_owned(),
        base_oid: String::new(),
        from_fork: false,
        sha: "abc1234".to_owned(),
        author: String::new(),
        url: String::new(),
        draft: false,
    }];
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Impact)), &ctx);
    assert!(!matches!(
        tab.impact_default_ask(),
        crate::insights::impact::Ask::Pr { .. }
    ));
    if let Some(view) = &mut tab.insights {
        view.pr = Some(7);
    }
    assert_eq!(
        tab.impact_default_ask(),
        crate::insights::impact::Ask::Pr {
            number: 7,
            head: "abc1234".to_owned(),
            base: "main".to_owned(),
        }
    );
}
