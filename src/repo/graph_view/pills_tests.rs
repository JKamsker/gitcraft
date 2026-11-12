use super::*;
use crate::git::{Branch, Refs};

#[test]
fn primary_and_stacked_branch_pills_select_details_without_row_selection() {
    use crate::repo::graph_view::readability_tests::painted_text_center;
    use egui::{Event, Modifiers, PointerButton};
    for stacked in [false, true] {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/recording-only/pill-click".into(), &ctx);
        tab.commits.push(std::sync::Arc::new(crate::git::Commit {
            hash: "a".repeat(40),
            refs: vec!["topic".into()],
            ..Default::default()
        }));
        tab.graph = crate::graph::layout(&tab.commits);
        let badge = Badge {
            local: true,
            ..remote_pill("topic", "origin")
        };
        let mut frame = |events| {
            let mut acts = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        if stacked {
                            let resp = ui.button("topic");
                            tab.stack_slot_clicks(resp, &badge, 0, &mut acts);
                        } else {
                            tab.graph_view(ui, &mut acts);
                        }
                    });
                },
            );
            output.textures_delta.clear();
            (acts, output.shapes)
        };
        frame(Vec::new());
        let (_, shapes) = frame(Vec::new());
        let point = painted_text_center(&shapes, "topic");
        frame(vec![Event::PointerMoved(point)]);
        let button = |pressed| Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(vec![button(true)]);
        let (acts, _) = frame(vec![button(false)]);
        assert_eq!(
            acts.iter()
                .filter(|a| matches!(a, Action::SelectBranchTip(0)))
                .count(),
            1
        );
        assert!(
            !acts
                .iter()
                .any(|a| matches!(a, Action::Select(_) | Action::OpenCommitChanges(_)))
        );
    }
}

fn tab(refs: Refs) -> RepoTab {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pills-test".into(), &ctx);
    tab.refs = refs;
    tab.rebuild_graph_labels();
    tab
}

fn remote_pill(label: &str, remote: &str) -> Badge {
    Badge {
        label: label.to_owned(),
        kind: BadgeKind::Branch,
        local: false,
        remotes: vec![remote.to_owned()],
        head: false,
    }
}

#[test]
fn white_pill_labels_meet_aa_for_every_builtin_and_lane_setting() {
    use crate::theme::{LaneColors, Tweaks, catalog::BUILTIN, palette::contrast};
    for builtin in BUILTIN {
        for lanes in LaneColors::ALL {
            let mut t = builtin.palette().tokens();
            Tweaks {
                lanes,
                ..Tweaks::default()
            }
            .apply(&mut t);
            for lane in t.lanes {
                for head in [false, true] {
                    for (kind, local) in [
                        (BadgeKind::Branch, true),
                        (BadgeKind::Branch, false),
                        (BadgeKind::Tag, false),
                        (BadgeKind::DetachedHead, false),
                    ] {
                        let badge = Badge {
                            label: "test".to_owned(),
                            kind,
                            local,
                            remotes: vec![],
                            head,
                        };
                        assert!(
                            contrast(Color32::WHITE, pill_fill(&t, &badge, lane)) >= 4.5,
                            "{}: {:?}, head={head}, lanes={lanes:?}",
                            builtin.name,
                            badge.kind
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn minimum_column_keeps_dev_and_the_extra_refs_count_visible() {
    let mut tab = tab(Refs {
        current: Some("dev".to_owned()),
        ..Refs::default()
    });
    tab.prs.push(crate::forge::PullRequest {
        number: 1,
        title: "Dev changes".to_owned(),
        branch: "dev".to_owned(),
        base: "main".to_owned(),
        base_oid: String::new(),
        from_fork: false,
        sha: "a".to_owned(),
        author: String::new(),
        url: String::new(),
        draft: false,
    });
    tab.ci.by_sha.insert(
        "a".to_owned(),
        crate::ci::Status {
            state: CheckState::Passed,
            checks: Vec::new(),
        },
    );
    let commit = crate::git::Commit {
        hash: "a".to_owned(),
        refs: ["HEAD -> dev", "refs/remotes/origin/dev", "tag: v1"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ..crate::git::Commit::default()
    };
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let row = Row {
            i: 0,
            commit: &commit,
            rect: Rect::from_min_size(Pos2::ZERO, vec2(600.0, 32.0)),
            cols: Cols {
                refs: crate::repo::columns::MIN_REFS_W,
                graph: 60.0,
                hash: 0.0,
                author: 0.0,
                when: 0.0,
                date: 0.0,
                push: 0.0,
            },
            gx: crate::repo::columns::MIN_REFS_W,
            lane_w: 20.0,
            node_x: Some(90.0),
            color: Color32::BLUE,
        };
        let resp = ui.interact(row.rect, ui.id().with("narrow-refs"), Sense::click());
        let (_, hit) = tab.refs_cell(ui, ui.painter(), &row, resp, &mut Vec::new());
        assert!(hit.is_some_and(|hit| hit.rect.right() <= row.gx - 4.0));
    });
    let mut names = Vec::new();
    for shape in &output.shapes {
        if let egui::epaint::Shape::Text(text) = &shape.shape {
            let label = text.galley.text();
            if label == "dev" || label == "+1" {
                assert!(text.pos.x >= shape.clip_rect.left());
                assert!(text.pos.x + text.galley.size().x <= shape.clip_rect.right());
                names.push(label);
            }
        }
    }
    assert!(
        names.contains(&"dev"),
        "the short branch name must stay whole"
    );
    assert!(
        names.contains(&"+1"),
        "the extra refs count must stay reachable"
    );
    output.textures_delta.clear();
}

#[test]
fn only_the_tracked_remote_pill_of_the_checked_out_branch_pulls() {
    let main = Branch {
        name: "main".to_owned(),
        target: "a".to_owned(),
        is_head: true,
        upstream: "origin/main".to_owned(),
        track: "[behind 2]".to_owned(),
    };
    let tab = tab(Refs {
        local: vec![main],
        current: Some("main".to_owned()),
        head: "a".to_owned(),
        ..Refs::default()
    });
    assert!(matches!(
        tab.pill_checkout(&remote_pill("main", "origin")),
        Some(Action::Pull)
    ));
    // `git pull` would bring in origin/main, not fork/main.
    assert!(tab.pill_checkout(&remote_pill("main", "fork")).is_none());
}
