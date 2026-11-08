//! Check the painted geometry and interactions, not just the preferred-width arithmetic.

use std::sync::Arc;

use egui::epaint::{ClippedShape, TextShape};
use egui::{Event, Pos2, Rect, Shape, vec2};

use super::*;
use crate::{git, theme};

fn fixture(count: usize) -> (egui::Context, RepoTab) {
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/graph-readability-test".into(), &ctx);
    tab.commits = (0..count)
        .map(|i| {
            Arc::new(git::Commit {
                hash: format!("{i:040x}"),
                parents: if i + 1 < count {
                    vec![format!("{:040x}", i + 1)]
                } else {
                    vec![]
                },
                summary: format!(
                    "Commit {i}: {}",
                    "Unicode 界 café 🦀 message with a long subject ".repeat(6)
                ),
                body: "A complete body.\n\nAnother paragraph with details.".to_owned(),
                refs: if i == 0 {
                    vec!["HEAD -> main".to_owned()]
                } else {
                    vec![]
                },
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    (ctx, tab)
}

fn frame(
    ctx: &egui::Context,
    tab: &mut RepoTab,
    size: egui::Vec2,
    events: Vec<Event>,
    time: f64,
) -> (Vec<ClippedShape>, Vec<Action>) {
    let mut acts = Vec::new();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(time),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut acts));
        },
    );
    out.textures_delta.clear();
    (std::mem::take(&mut out.shapes), acts)
}

fn texts(shapes: &[ClippedShape]) -> Vec<&TextShape> {
    fn visit<'a>(shape: &'a Shape, out: &mut Vec<&'a TextShape>) {
        match shape {
            Shape::Text(text) => out.push(text),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in shapes {
        visit(&shape.shape, &mut out);
    }
    out
}

/// Locate the visible ink actually painted by the graph, including any headers above it.
pub(super) fn painted_text_center(shapes: &[ClippedShape], label: &str) -> Pos2 {
    let matching: Vec<_> = texts(shapes)
        .into_iter()
        .filter(|text| text.galley.job.text == label)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "missing or duplicated painted text: {label}"
    );
    let bounds = matching[0].visual_bounding_rect();
    assert!(bounds.is_positive(), "empty painted text: {label}");
    bounds.center()
}

#[test]
fn topology_arrows_enter_history_from_uncommitted_changes_then_follow_parents() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).hash = WIP_HASH.into();
    tab.refs.head = tab.commits[1].hash.clone();
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(row, commit)| (commit.hash.clone(), row))
        .collect();
    tab.topology = Arc::new(crate::repo::graph_walk::Topology::build(
        &tab.commits,
        &tab.index,
        &Default::default(),
    ));
    tab.graph = crate::graph::layout(&tab.commits);
    tab.selected = Some(0);
    tab.rebuild_graph_labels();
    Settings {
        graph_lane_walk: true,
        ..Default::default()
    }
    .set(&ctx);
    let size = vec2(900.0, 600.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let key = |pressed| Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let (_, acts) = frame(&ctx, &mut tab, size, vec![key(true)], 0.1);
    assert!(acts.iter().any(|act| matches!(act, Action::Select(1))));
    // Apply only the selected cursor; opening commit details would start Git reads
    // against the deliberately nonexistent fixture repository.
    tab.selected = Some(1);
    frame(&ctx, &mut tab, size, vec![key(false)], 0.2);
    let (_, acts) = frame(&ctx, &mut tab, size, vec![key(true)], 0.3);
    assert!(acts.iter().any(|act| matches!(act, Action::Select(2))));

    // A stash also has no edges in the walking topology, but must not trap focus.
    let stash_hash = "stash-fixture".to_owned();
    Arc::make_mut(&mut tab.commits[0]).hash = stash_hash.clone();
    tab.stashes.push(git::Stash {
        name: "stash@{0}".into(),
        hash: stash_hash.clone(),
        base: tab.refs.head.clone(),
        message: "saved work".into(),
        author: String::new(),
        email: String::new(),
        time: 0,
        date: String::new(),
    });
    tab.index.remove(WIP_HASH);
    tab.index.insert(stash_hash, 0);
    tab.selected = Some(0);
    tab.rebuild_graph_labels();
    frame(&ctx, &mut tab, size, vec![key(false)], 0.4);
    let (_, acts) = frame(&ctx, &mut tab, size, vec![key(true)], 0.5);
    assert!(acts.iter().any(|act| matches!(act, Action::Select(1))));
}

#[test]
fn topology_arrows_follow_the_focused_row_when_nothing_is_selected() {
    let (ctx, mut tab) = fixture(10);
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(row, commit)| (commit.hash.clone(), row))
        .collect();
    tab.topology = Arc::new(crate::repo::graph_walk::Topology::build(
        &tab.commits,
        &tab.index,
        &Default::default(),
    ));
    tab.selected = None;
    Settings {
        graph_lane_walk: true,
        ..Default::default()
    }
    .set(&ctx);
    let mut acts = Vec::new();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![Event::Key {
                key: Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| tab.arrow_keys(ui, Some(5), &mut acts),
    );
    output.textures_delta.clear();
    assert!(acts.iter().any(|act| matches!(act, Action::Select(6))));
}

fn header_x(shapes: &[ClippedShape], label: &str) -> f32 {
    let matching: Vec<_> = texts(shapes)
        .into_iter()
        .filter(|s| s.galley.job.text == label)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "missing or duplicated painted header: {label}"
    );
    matching[0].pos.x
}

// Locate rows from painted content: the history filter strip can move the graph vertically.
fn summary_y(shapes: &[ClippedShape], summary: &str) -> f32 {
    let found = texts(shapes)
        .into_iter()
        .find(|t| t.galley.job.text == summary);
    assert!(found.is_some(), "missing painted commit summary: {summary}");
    found.map_or(0.0, |text| text.visual_bounding_rect().center().y)
}

#[test]
fn scrolling_reclaims_empty_refs_without_width_jitter_or_full_history_paint() {
    let (ctx, mut tab) = fixture(10_000);
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (top, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    let with_pill = header_x(&top, "GRAPH");
    tab.scroll_to = Some(5000);
    let (scrolled, _) = frame(&ctx, &mut tab, size, vec![], 0.2);
    assert!(header_x(&scrolled, "GRAPH") < with_pill);
    assert_eq!(
        header_x(&scrolled, "GRAPH"),
        MIN_REFS_W + crate::repo::columns::CELL_PAD
    );
    assert!(texts(&scrolled).iter().any(|s| s.galley.job.text == "main"
        && s.galley.job.sections[0].format.color == Tokens::get(&ctx).text_faint));
    let painted_label = scrolled.iter().find(|clipped| {
        matches!(&clipped.shape,
        Shape::Text(text) if text.galley.job.text == "main")
    });
    assert!(
        painted_label.is_some_and(|clipped| match &clipped.shape {
            Shape::Text(text) => clipped
                .clip_rect
                .contains(text.visual_bounding_rect().center()),
            _ => false,
        }),
        "the lane name must be visible, not merely painted above the viewport clip"
    );
    let summaries: Vec<_> = texts(&scrolled)
        .into_iter()
        .filter(|s| s.galley.job.text.starts_with("Commit "))
        .collect();
    assert!(
        !summaries.is_empty() && summaries.len() < 12,
        "history must remain virtualized"
    );
    for text in summaries {
        assert!(text.galley.elided);
        assert!(
            text.galley
                .rows
                .iter()
                .flat_map(|r| &r.glyphs)
                .any(|g| g.chr == '…')
        );
        assert!(
            text.visual_bounding_rect().right()
                <= size.x
                    - preferred_cols(&Settings::default(), lane_layout(&Settings::default(), 1))
                        .trailing()
        );
    }
    let (stable, _) = frame(&ctx, &mut tab, size, vec![], 0.3);
    assert_eq!(
        header_x(&stable, "DESCRIPTION"),
        header_x(&scrolled, "DESCRIPTION")
    );
    assert_eq!(Settings::get(&ctx), Settings::default());
}

#[test]
fn manual_refs_optional_columns_and_compact_graph_survive_auto_sizing() {
    let (ctx, mut tab) = fixture(100);
    let settings = Settings {
        refs_width: 310.0,
        compact_graph: true,
        show_hash: true,
        show_date: true,
        show_when: true,
        show_author: true,
        ..Default::default()
    };
    settings.clone().set(&ctx);
    let size = vec2(1000.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    tab.scroll_to = Some(50);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    // This full-height fixture reserves 84 points for the minimap.
    let available = size.x - 84.0;
    let shown = preferred_cols(&settings, lane_layout(&settings, 1))
        .squeezed(available, settings.last_resized);
    assert_eq!(
        header_x(&shapes, "GRAPH"),
        shown.refs + crate::repo::columns::CELL_PAD
    );
    for name in ["COMMIT", "AUTHOR", "WHEN", "DATE"] {
        header_x(&shapes, name);
    }
    frame(&ctx, &mut tab, vec2(220.0, 220.0), vec![], 0.2);
    assert_eq!(Settings::get(&ctx), settings);
}

#[test]
fn real_git_tooltip_retains_long_message_paragraphs_and_follows_hover_not_selection()
-> Result<(), String> {
    let path = std::env::temp_dir().join(format!("gitcraft-graph-message-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| git::run(Some(&path), args);
    git(&["init", "-q"])?;
    git(&["config", "user.name", "Graph test"])?;
    git(&["config", "user.email", "graph@example.invalid"])?;
    git(&["config", "commit.gpgSign", "false"])?;
    git(&["commit", "--allow-empty", "-qm", "Base"])?;
    let other = "Previous subject\n\nThe previous commit has its own full body.";
    git(&["commit", "--allow-empty", "-qm", other])?;
    let message = format!(
        "{}\n\n{}\n\nFinal paragraph: TAIL THAT MUST BE VISIBLE.",
        "Long Unicode café subject that will be elided in the graph ".repeat(3),
        "A detailed explanation with spacing and Unicode café. ".repeat(20)
    );
    let message_path = path.join("message.txt");
    std::fs::write(&message_path, &message).map_err(|e| e.to_string())?;
    git(&[
        "commit",
        "--allow-empty",
        "-q",
        "--cleanup=verbatim",
        "-F",
        &message_path.to_string_lossy(),
    ])?;
    // Use production Git and parse_record, rather than injecting a synthetic Commit.body.
    let commits = git::log(&path, None, Default::default(), 0, 10)?;
    assert_eq!(commits[0].body.chars().count(), 400);
    assert!(!commits[0].body.contains("TAIL THAT MUST BE VISIBLE"));
    let first = commits[0].hash.clone();
    let second = commits[1].hash.clone();
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(path.clone(), &ctx);
    tab.commits = commits.into_iter().map(Arc::new).collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    tab.selected = Some(2);
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    let first_point = painted_text_center(&shapes, &tab.commits[0].summary);
    let second_point = painted_text_center(&shapes, &tab.commits[1].summary);
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        0.2,
    );
    frame(&ctx, &mut tab, size, vec![], 0.65); // Start the background request.
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(second_point)],
        0.7,
    );
    let wait_for = |tab: &mut RepoTab, hash: &str| -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while tab.graph_messages.message(&path, hash).is_none() {
            // Accept the real tooltip worker's result without starting unrelated readers.
            while let Ok(msg) = tab.rx.try_recv() {
                tab.handle(msg, &ctx);
            }
            if std::time::Instant::now() >= deadline {
                return Err("message worker did not finish".to_owned());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(())
    };
    wait_for(&mut tab, &first)?;
    assert!(
        matches!(tab.graph_messages.message(&path, &first), Some(Ok(text)) if text == &message)
    );
    let (loading_second, _) = frame(&ctx, &mut tab, size, vec![], 1.5);
    assert!(
        !texts(&loading_second)
            .iter()
            .any(|s| s.galley.job.text == message),
        "an old hover result must never appear over the next row"
    );
    wait_for(&mut tab, &second)?;
    let (second_tooltip, _) = frame(&ctx, &mut tab, size, vec![], 2.0);
    assert!(
        texts(&second_tooltip)
            .iter()
            .any(|s| s.galley.job.text == other)
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        2.1,
    );
    frame(&ctx, &mut tab, size, vec![], 2.8);
    let (first_tooltip, _) = frame(&ctx, &mut tab, size, vec![], 3.2);
    assert!(
        texts(&first_tooltip)
            .iter()
            .any(|s| s.galley.job.text == message)
    );
    let message_geometry = |shapes: &[ClippedShape]| {
        shapes.iter().find_map(|clipped| match &clipped.shape {
            Shape::Text(text) if text.galley.job.text == message => {
                let tail = text
                    .galley
                    .rows
                    .last()?
                    .rect()
                    .translate(text.pos.to_vec2());
                Some((clipped.clip_rect, tail))
            }
            _ => None,
        })
    };
    let (clip, tail) = message_geometry(&first_tooltip).ok_or("missing message geometry")?;
    assert!(
        !clip.contains_rect(tail),
        "fixture must need scrolling to reach its tail"
    );
    let point = clip
        .intersect(Rect::from_min_size(Pos2::ZERO, size))
        .center();
    frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], 3.21);
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: vec2(0.0, -4000.0),
            modifiers: egui::Modifiers::NONE,
        }],
        3.22,
    );
    let (scrolled_tooltip, _) = frame(&ctx, &mut tab, size, vec![], 3.25);
    let (clip, tail) =
        message_geometry(&scrolled_tooltip).ok_or("tooltip closed while scrolling")?;
    assert!(
        clip.contains_rect(tail),
        "final paragraph must be reachable: {tail:?} {clip:?}"
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        3.26,
    );
    assert_eq!(
        tab.selected,
        Some(2),
        "hover must not change the selected commit"
    );
    // Leave the scrollable popup before clicking its anchor: in this short viewport
    // the popup can cover the row. Re-enter and click before its hover delay elapses.
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(Pos2::new(1.0, 1.0))],
        3.3,
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        3.6,
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerButton {
            pos: first_point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        3.61,
    );
    let (_, acts) = frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerButton {
            pos: first_point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        3.62,
    );
    assert!(acts.iter().any(|a| matches!(a, Action::Select(0))));
    drop(tab);
    assert!(git::commit_message(&path, "--output=written").is_err());
    std::fs::write(
        &message_path,
        "x".repeat(git::MAX_TOOLTIP_MESSAGE_BYTES + 1),
    )
    .map_err(|e| e.to_string())?;
    git(&[
        "commit",
        "--allow-empty",
        "-q",
        "--cleanup=verbatim",
        "-F",
        &message_path.to_string_lossy(),
    ])?;
    let oversized = git(&["rev-parse", "HEAD"])?;
    assert!(
        git::commit_message(&path, oversized.trim()).is_err_and(|error| error.contains("256 KiB")),
        "oversized objects must be refused before retaining full message text"
    );
    let _ = std::fs::remove_dir_all(&path);
    Ok(())
}

#[test]
fn lane_names_distinguish_refs_tags_detached_and_reused_columns() {
    let (ctx, mut tab) = fixture(8);
    tab.commits[3] = Arc::new(git::Commit {
        hash: "unrelated".into(),
        parents: vec!["other".into()],
        refs: vec!["origin/topic".into()],
        ..Default::default()
    });
    tab.commits[4] = Arc::new(git::Commit {
        hash: "other".into(),
        parents: vec!["tail".into()],
        ..Default::default()
    });
    tab.commits[5] = Arc::new(git::Commit {
        hash: "tail".into(),
        ..Default::default()
    });
    tab.commits[6] = Arc::new(git::Commit {
        hash: "tagged".into(),
        refs: vec!["tag: v1".into()],
        ..Default::default()
    });
    tab.commits[7] = Arc::new(git::Commit {
        hash: "detached".into(),
        ..Default::default()
    });
    tab.refs.remotes = vec![("origin".into(), vec![])];
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    ctx.run_ui(Default::default(), |ui| {
        let named = tab.readable_viewport(ui, 1..3, 24);
        assert_eq!(named.labels.len(), 1);
        assert_eq!(named.labels[&1], "main");
        // A visible real pill suppresses the faint duplicate, even if it is a tag.
        assert!(tab.readable_viewport(ui, 3..5, 24).labels.is_empty());
        let tip_offscreen = tab.readable_viewport(ui, 4..6, 24);
        assert_eq!(tip_offscreen.labels[&4], "origin/topic");
        let tag_visible = tab.readable_viewport(ui, 6..7, 24);
        assert!(tag_visible.labels.is_empty());
        let detached = tab.readable_viewport(ui, 7..8, 24);
        assert_eq!(detached.labels[&7], "detached");
    })
    .drop_without_applying_deltas();
}

#[test]
fn auto_sized_pills_keep_double_click_and_branch_context_hitboxes() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).refs = vec!["feature".to_owned()];
    tab.rebuild_graph_labels();
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.05);
    let point = painted_text_center(&shapes, "feature");
    frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], 0.1);
    let mut actions = Vec::new();
    for (n, pressed) in [true, false, true, false].into_iter().enumerate() {
        let (_, acts) = frame(
            &ctx,
            &mut tab,
            size,
            vec![Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            0.2 + n as f64 * 0.03,
        );
        actions.extend(acts);
    }
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Checkout(name) if name == "feature"))
    );
    let events = [true, false]
        .into_iter()
        .map(|pressed| Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    frame(&ctx, &mut tab, size, events, 0.6);
    let hash = &tab.commits[0].hash;
    assert_eq!(
        ctx.data(|d| d.get_temp::<Option<String>>(super::menu::pill_menu_id(hash))),
        Some(Some("feature".to_owned()))
    );
    assert!(egui::Popup::is_any_open(&ctx));
}

#[test]
fn ghost_rows_do_not_offer_an_unresolvable_message_popup() {
    let (ctx, mut tab) = fixture(10);
    let created = git::Commit {
        hash: "\0ghost-test".into(),
        summary: "Preview only".into(),
        parents: vec![tab.commits[0].hash.clone()],
        ..Default::default()
    };
    tab.ghost.show_for_test(
        crate::repo::ghost::Outcome {
            created: vec![created],
            copies: 1,
            ..Default::default()
        },
        &tab.commits,
    );
    let size = vec2(900.0, 220.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.0);
    let point = egui::pos2(
        header_x(&shapes, "DESCRIPTION") + 35.0,
        summary_y(&shapes, "Preview only"),
    );
    for time in [0.1, 1.0, 2.0] {
        let (shapes, _) = frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], time);
        assert!(
            !texts(&shapes)
                .iter()
                .any(|t| t.galley.text().contains("Loading full commit message"))
        );
    }
    assert!(
        tab.graph_messages
            .message(&tab.path, "\0ghost-test")
            .is_none()
    );
}

#[test]
fn cached_badges_refresh_when_refs_change_even_without_changing_the_graph() {
    let (ctx, mut tab) = fixture(1);
    let commit = git::Commit {
        refs: vec!["origin/topic".into()],
        ..Default::default()
    };
    tab.refs.remotes = vec![("origin".into(), vec![])];
    tab.rebuild_graph_labels();
    let first = tab.graph_badges(&commit);
    assert_eq!(first[0].label, "topic");
    assert!(!first[0].local);
    tab.refs.remotes.clear();
    tab.refs.local = vec![git::Branch {
        name: "origin/topic".into(),
        is_head: true,
        target: String::new(),
        upstream: String::new(),
        track: String::new(),
    }];
    tab.refs.current = Some("origin/topic".into());
    tab.rebuild_graph_labels();
    let refreshed = tab.graph_badges(&commit);
    assert_eq!(refreshed[0].label, "origin/topic");
    assert!(refreshed[0].local && refreshed[0].head && refreshed[0].remotes.is_empty());
    frame(&ctx, &mut tab, vec2(900.0, 220.0), vec![], 0.0);
}

#[test]
fn hidden_remote_names_are_not_reintroduced_by_faint_lane_labels() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).refs = vec!["refs/remotes/origin/hidden".into()];
    tab.visibility.toggle_remote_graph("origin");
    tab.rebuild_graph_labels();
    ctx.run_ui(Default::default(), |ui| {
        let viewport = tab.readable_viewport(ui, 1..3, 24);
        assert!(
            viewport
                .labels
                .values()
                .all(|name| !name.contains("hidden"))
        );
    })
    .drop_without_applying_deltas();
}
