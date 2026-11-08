//! The review heatmap in a tab: without a forge, and drawing a pull request read from a real
//! repository, its cells opening the diff at the hunk.

use std::time::{Duration, Instant};

use super::Act;
use crate::forge::{Forge, Hosting};
use crate::insights::review_heat::{
    self as heat,
    tests::{KEY, details, pr_repo},
};
use crate::repo::insights::tests::draw;
use crate::repo::insights::{InsightsAct, Kind};
use crate::repo::{Action, DiffKey, RepoTab};

fn both_ways(tab: &mut RepoTab, ctx: &egui::Context) {
    draw(tab, ctx, 1200.0);
    draw(tab, ctx, 600.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
    draw(tab, ctx, 1200.0);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), ctx);
}

#[test]
fn without_a_forge_the_view_says_so() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-review-heat".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::ReviewHeat)), &ctx);
    both_ways(&mut tab, &ctx);
    // Picking one without a forge reads nothing.
    tab.apply(
        Action::Insights(InsightsAct::ReviewHeat(Act::Pick(3))),
        &ctx,
    );
    let state = tab.insights.as_ref().map(|v| &v.review_heat);
    assert!(state.is_some_and(|s| s.number == Some(3) && !s.data.loading));
    both_ways(&mut tab, &ctx);
    assert!(!tab.review_heat_back());
}

#[test]
fn a_pull_request_is_drawn_and_a_hunk_opens_at_its_line() -> Result<(), String> {
    let (repo, base, head) = pr_repo("review-heat-tab")?;
    // A push since the version seen: line 25 changed.
    let lines: String = (1..=30)
        .map(|n| match n {
            3 => "line three\n".to_owned(),
            25 => "line twenty-five\n".to_owned(),
            _ => format!("line {n}\n"),
        })
        .collect();
    let moved = repo.commit("edit a again", &[("a.txt", Some(&lines))])?;
    let data = heat::analyze(repo.path(), &details(&base, &moved), Some(KEY))?;
    assert_eq!(data.seen_tip(), Some(head.as_str()));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    tab.hosting = Some(Hosting {
        forge: Forge::GitHub,
        host: "example.org".into(),
        path: "o/r".into(),
        web: "https://example.org".into(),
    });
    tab.apply(Action::Insights(InsightsAct::Open(Kind::ReviewHeat)), &ctx);
    let state = tab
        .insights
        .as_mut()
        .map(|v| &mut v.review_heat)
        .ok_or("no page")?;
    state.number = Some(5);
    let request = state.data.start();
    state.data.accept(request, Ok(data));
    both_ways(&mut tab, &ctx);
    tab.apply(
        Action::Insights(InsightsAct::ReviewHeat(Act::ExpandAll(true))),
        &ctx,
    );
    both_ways(&mut tab, &ctx);
    // The hunk changed since opens from the version seen, at its line.
    let key = DiffKey::Range {
        from: head.clone(),
        to: Some(moved.clone()),
        path: "a.txt".into(),
    };
    tab.apply(
        Action::Insights(InsightsAct::ReviewHeat(Act::Open {
            key: key.clone(),
            at: Some((false, 25)),
            cell: Some((2, 0)),
        })),
        &ctx,
    );
    let start = Instant::now();
    while tab.diff.as_ref().is_none_or(|d| d.jump_line.is_none()) {
        if start.elapsed() > Duration::from_secs(20) {
            return Err("the diff never scrolled to its line".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(&ctx);
    }
    let view = tab.diff.as_ref().ok_or("no diff")?;
    assert_eq!(view.key, key);
    let line = view
        .data
        .as_ref()
        .and_then(|d| d.as_ref().ok())
        .zip(view.jump_line)
        .map(|(d, i)| d.lines[i].new);
    assert_eq!(line, Some(Some(25)));
    // Esc takes back the cell opened, then closes the page.
    assert!(tab.review_heat_back());
    assert!(!tab.review_heat_back());
    Ok(())
}
