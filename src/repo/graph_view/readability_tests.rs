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
    assert_eq!(header_x(&scrolled, "GRAPH"), MIN_REFS_W + 10.0);
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
    let shown = preferred_cols(&settings, lane_layout(&settings, 1))
        .squeezed(size.x, settings.last_resized);
    assert_eq!(header_x(&shapes, "GRAPH"), shown.refs + 10.0);
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
    let commits = git::log(&path, None, 0, 10)?;
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
    let x = header_x(&shapes, "DESCRIPTION") + 30.0;
    let first_y = texts(&shapes)
        .into_iter()
        .find(|s| s.galley.job.text == tab.commits[0].summary)
        .ok_or("First commit description was not painted")?
        .visual_bounding_rect()
        .center()
        .y;
    let first_point = egui::pos2(x, first_y);
    let second_point = first_point + vec2(0.0, ROW_PITCH);
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
            tab.poll(&ctx);
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
    assert_eq!(
        tab.selected,
        Some(2),
        "hover must not change the selected commit"
    );
    let events = [true, false]
        .into_iter()
        .map(|pressed| Event::PointerButton {
            pos: first_point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    let (_, acts) = frame(&ctx, &mut tab, size, events, 3.3);
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
fn auto_sized_pills_keep_double_click_and_branch_context_hitboxes() -> Result<(), String> {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).refs = vec!["feature".to_owned()];
    tab.rebuild_graph_labels();
    let size = vec2(900.0, 220.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.0);
    let point = texts(&shapes)
        .into_iter()
        .find(|s| s.galley.job.text == "feature")
        .ok_or("The feature branch pill was not painted")?
        .visual_bounding_rect()
        .center();
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
    Ok(())
}

/// Source-only frame fixture: topology keys keep screen-reader focus and never invoke Git.
#[test]
fn topology_keyboard_frame_skips_unrelated_rows_and_keeps_focus() {
    let (ctx, mut tab) = fixture(6);
    Settings {
        graph_lane_walk: true,
        avatars: false,
        ..Default::default()
    }
    .set(&ctx);
    let parent = tab.commits[3].hash.clone();
    Arc::make_mut(&mut tab.commits[0]).parents = vec![parent];
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.topology = Arc::new(crate::repo::graph_walk::Topology::build(
        &tab.commits,
        &tab.index,
        &std::collections::HashSet::new(),
    ));
    tab.selected = Some(0);
    let key = |key| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&ctx, &mut tab, vec2(1100.0, 600.0), vec![], 0.0);
    let (_, actions) = frame(
        &ctx,
        &mut tab,
        vec2(1100.0, 600.0),
        vec![key(Key::ArrowDown)],
        0.1,
    );
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Select(3)))
    );
    tab.selected = Some(3);
    frame(&ctx, &mut tab, vec2(1100.0, 600.0), vec![], 0.2);
    assert!(tab.graph_walk_focus_id.is_some());
    assert_eq!(ctx.memory(|m| m.focused()), tab.graph_walk_focus_id);
    let (_, actions) = frame(
        &ctx,
        &mut tab,
        vec2(1100.0, 600.0),
        vec![key(Key::ArrowDown)],
        0.3,
    );
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Select(4)))
    );
    tab.keys_blocked = true;
    let (_, actions) = frame(
        &ctx,
        &mut tab,
        vec2(1100.0, 600.0),
        vec![key(Key::ArrowDown)],
        0.4,
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Select(_)))
    );
}
