use super::*;
use crate::git;

#[test]
fn a_selected_pr_branch_is_visible_ahead_of_other_branches_at_its_tip() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-pill-focus".into(), &ctx);
    let commit = git::Commit {
        hash: "head".into(),
        refs: vec!["HEAD -> dev".into(), "feature".into()],
        ..git::Commit::default()
    };
    tab.compare = Some(crate::repo::Compare {
        base: "main".into(),
        base_label: "main".into(),
        target: Some("head".into()),
        target_label: "feature".into(),
        captions: [None, None],
        since_fork: true,
        data: None,
        request: 1,
    });
    let focused = tab.graph_badges(&commit);
    assert_eq!(focused[0].label, "feature");
    assert_eq!(focused[1].label, "dev");
    tab.prs.push(crate::forge::PullRequest {
        number: 7,
        title: String::new(),
        branch: "fork-feature".into(),
        base: "main".into(),
        base_oid: String::new(),
        sha: "head".into(),
        from_fork: true,
        author: String::new(),
        url: String::new(),
        draft: false,
    });
    if let Some(compare) = &mut tab.compare {
        compare.target_label = "fork-feature".into();
    }
    let focused = tab.graph_badges(&commit);
    assert_eq!(focused[0].label, "fork-feature");
    assert_eq!(focused[0].kind, BadgeKind::PullRequest);
    assert!(focused[0].full_refs().is_empty());
    assert!(!focused[0].local && focused[0].remotes.is_empty());
}

#[test]
fn a_compact_graph_column_fits_its_lanes_closer_together() {
    let wide = Settings::default();
    let compact = Settings {
        compact_graph: true,
        ..Settings::default()
    };
    let graph = |s: &Settings| preferred_cols(s, lane_layout(s, 10)).graph;
    assert_eq!(graph(&wide), 172.0);
    assert_eq!(graph(&compact), 112.0);
    // A width dragged by hand stays.
    let dragged = Settings {
        graph_width: 150.0,
        ..compact
    };
    assert_eq!(graph(&dragged), 150.0);
}

/// The graph of `tab` showing the commits `hashes` (newest first, each on the next).
fn show_commits(tab: &mut RepoTab, hashes: &[&str]) {
    tab.commits = hashes
        .iter()
        .enumerate()
        .map(|(i, h)| {
            std::sync::Arc::new(git::Commit {
                hash: (*h).to_owned(),
                parents: hashes
                    .get(i + 1)
                    .map(|p| (*p).to_owned())
                    .into_iter()
                    .collect(),
                summary: format!("commit {h}"),
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.index = hashes
        .iter()
        .enumerate()
        .map(|(i, h)| ((*h).to_owned(), i))
        .collect();
}

/// A commit's menu stays open while a ghost run's rows push its row out of view (the
/// entry under the pointer is what is previewed), and the ghost graph draws in its place.
#[test]
fn a_menu_stays_open_while_ghost_rows_push_its_row_away() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/ghost-menu-test".into(), &ctx);
    let hashes: Vec<String> = ["c", "b", "a"].iter().map(|h| h.repeat(40)).collect();
    let refs: Vec<&str> = hashes.iter().map(String::as_str).collect();
    show_commits(&mut tab, &refs);
    // A short window: the header and a few rows.
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 160.0));
    let second = egui::pos2(600.0, 52.0 + ROW_PITCH * 1.5);
    let frame = |tab: &mut RepoTab, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
        })
        .drop_without_applying_deltas();
    };
    frame(&mut tab, vec![egui::Event::PointerMoved(second)]);
    let click = |pressed| egui::Event::PointerButton {
        pos: second,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut tab, vec![click(true), click(false)]);
    frame(&mut tab, Vec::new());
    let opened = egui::Popup::is_any_open(&ctx);
    // Twenty commits on top: the right-clicked row is far below the view now.
    let created = (0..20)
        .map(|n| git::Commit {
            hash: format!("{n:040}"),
            parents: vec![if n == 19 {
                hashes[0].clone()
            } else {
                format!("{:040}", n + 1)
            }],
            summary: format!("copy {n}"),
            ..Default::default()
        })
        .collect();
    let outcome = crate::repo::ghost::Outcome {
        created,
        copies: 20,
        ..Default::default()
    };
    let rows = tab.commits.clone();
    tab.ghost.show_for_test(outcome, &rows);
    // Near the end of what is loaded, with more to load: the ghost graph asks for none.
    tab.has_more = true;
    tab.selected = Some(2);
    let mut asked = Vec::new();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut asked));
        })
        .drop_without_applying_deltas();
    }
    assert!(!asked.iter().any(|a| matches!(a, Action::LoadMore)));
    assert_eq!(tab.selected, Some(2));
    assert!(opened, "the right-click opened no menu");
    assert!(egui::Popup::is_any_open(&ctx), "the menu closed");
    // The tab's own rows are back once the graph is drawn.
    assert_eq!(tab.commits.len(), 3);
    assert!(!tab.ghost.drawing());
}

/// A commit's menu left open while a reload replaces the rows is that commit's: when the
/// commit is gone (amended, or reset away in the terminal) it closes, rather than turning
/// into the menu of the commit that took its row, whose Revert or Cherry-pick runs at once.
#[test]
fn a_commit_menu_does_not_move_to_the_commit_that_takes_its_row() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/row-menu-test".into(), &ctx);
    show_commits(
        &mut tab,
        &["c".repeat(40).as_str(), &"b".repeat(40), &"a".repeat(40)],
    );
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0));
    // The header, then the rows: the second row's middle, away from its pills.
    let second = egui::pos2(600.0, 52.0 + ROW_PITCH * 1.5);
    let frame = |tab: &mut RepoTab, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
        })
        .drop_without_applying_deltas();
    };
    frame(&mut tab, vec![egui::Event::PointerMoved(second)]);
    let click = |pressed| egui::Event::PointerButton {
        pos: second,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut tab, vec![click(true), click(false)]);
    frame(&mut tab, Vec::new());
    let opened = egui::Popup::is_any_open(&ctx);
    // The right-clicked commit is gone: another one is in its row now.
    show_commits(&mut tab, &["d".repeat(40).as_str(), &"a".repeat(40)]);
    frame(&mut tab, Vec::new());
    frame(&mut tab, Vec::new());
    assert!(opened, "the right-click opened no menu");
    assert!(!egui::Popup::is_any_open(&ctx));
}
