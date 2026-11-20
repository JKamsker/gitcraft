//! The pills drawn by the real graph view: where they land, how each standing looks, what
//! the tooltip says, and that hiding them takes them away.

use std::sync::Arc;

use egui::epaint::{ClippedShape, TextShape};
use egui::{Event, Pos2, Rect, Shape, vec2};

use super::*;
use crate::ci::deployments::{DeployState, Deployment};
use crate::theme::Tokens;

const DAY: i64 = 24 * 3600;

fn hash(i: usize) -> String {
    format!("{i:040x}")
}

fn marker(env: &str, row: usize, reported: i64, succeeded: bool, preview: bool) -> Marker {
    Marker {
        environment: env.to_owned(),
        deployment: Deployment {
            environment: env.to_owned(),
            sha: hash(row),
            reference: "main".to_owned(),
            created: Some(reported),
            reported: Some(reported),
            state: Some(if succeeded {
                DeployState::Success
            } else {
                DeployState::NotRead
            }),
            ..Deployment::default()
        },
        succeeded,
        preview,
        digests: vec![("site".to_owned(), "sha256:feedface".to_owned())],
        present: Some(true),
        ..Marker::default()
    }
}

fn fixture(visible: bool) -> (egui::Context, RepoTab) {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut settings = crate::settings::Settings::get(&ctx);
    settings.show_deploy_markers = visible;
    settings.put_for_test(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/deploy-markers-test".into(), &ctx);
    tab.commits = (0..5)
        .map(|i| {
            Arc::new(crate::git::Commit {
                hash: hash(i),
                parents: if i < 4 { vec![hash(i + 1)] } else { vec![] },
                summary: format!("Commit number {i}"),
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    seed(&mut tab, &ctx);
    (ctx, tab)
}

/// The markers as a read for this tab left them, not due again. Seeded before every frame:
/// other tests change the process's connection generation, which retires markers (rightly).
fn seed(tab: &mut RepoTab, ctx: &egui::Context) {
    let now = crate::git::now();
    let key = tab.deploy_markers_key(ctx);
    tab.deploy_markers = State {
        markers: vec![
            marker("production", 1, now - 3600, true, false),
            marker("staging", 2, now - 30 * DAY, true, false),
            marker("qa", 3, now - 3600, false, false),
            marker("pr-7-preview", 1, now - 3600, true, true),
            marker("review/x", 1, now - 3600, true, true),
        ],
        observed: Some(now - 120),
        forge: Some(crate::forge::Forge::GitHub),
        key: Some(key),
        loaded: Some(Instant::now()),
        ..State::default()
    };
}

fn frame(
    ctx: &egui::Context,
    tab: &mut RepoTab,
    events: Vec<Event>,
    time: f64,
) -> Vec<ClippedShape> {
    seed(tab, ctx);
    let mut acts = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 260.0))),
        events,
        time: Some(time),
        ..Default::default()
    };
    let (mut out, _) = crate::repo::messages::record_jobs(|| {
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut acts));
        })
    });
    out.textures_delta.clear();
    std::mem::take(&mut out.shapes)
}

fn texts(shapes: &[ClippedShape]) -> Vec<&TextShape> {
    fn visit<'a>(shape: &'a Shape, out: &mut Vec<&'a TextShape>) {
        match shape {
            Shape::Text(text) => out.push(text),
            Shape::Vec(shapes) => shapes.iter().for_each(|s| visit(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|s| visit(&s.shape, &mut out));
    out
}

fn painted<'a>(shapes: &'a [ClippedShape], label: &str) -> Option<&'a TextShape> {
    texts(shapes)
        .into_iter()
        .find(|t| t.galley.job.text == label)
}

fn row_of(shapes: &[ClippedShape], label: &str) -> Option<f32> {
    painted(shapes, label).map(|t| t.visual_bounding_rect().center().y)
}

/// Runs `test` with this thread's account pinned: other tests switch the process's.
fn pinned<T>(test: impl FnOnce() -> T) -> T {
    crate::connections::Context::default().run(test)
}

#[test]
fn pills_land_on_their_commits_rows_with_each_standing_drawn_apart() {
    pinned(|| {
        let (ctx, mut tab) = fixture(true);
        frame(&ctx, &mut tab, vec![], 0.0);
        let shapes = frame(&ctx, &mut tab, vec![], 0.1);
        assert!(
            tab.deploy_markers.visible,
            "the setting reaches the markers"
        );
        assert_eq!(tab.deploy_markers.markers.len(), 5, "the markers are kept");
        let t = Tokens::get(&ctx);
        let row = |label: &str| row_of(&shapes, label).unwrap_or(f32::NAN);
        for (pill, summary) in [
            ("production", "Commit number 1"),
            ("2 previews", "Commit number 1"),
            ("staging", "Commit number 2"),
            ("qa ?", "Commit number 3"),
        ] {
            assert!(
                (row(pill) - row(summary)).abs() < 2.0,
                "{pill} must be on the row of {summary}"
            );
        }
        let ink = |label: &str| painted(&shapes, label).map(|t| t.fallback_color);
        let (current, stale, unknown) = (ink("production"), ink("staging"), ink("qa ?"));
        assert_eq!(
            (current, stale, unknown),
            (Some(t.text), Some(t.text_dim), Some(t.text_faint)),
            "current in full ink, stale dimmed, unknown faint with a question mark (summary ink {:?})",
            ink("Commit number 1")
        );
        // The previews are collapsed into one pill, not drawn one by one.
        assert!(painted(&shapes, "pr-7-preview").is_none());
    })
}

#[test]
fn the_tooltip_tells_source_age_digest_and_that_only_deployment_records_count() {
    pinned(|| {
        let (ctx, mut tab) = fixture(true);
        frame(&ctx, &mut tab, vec![], 0.0);
        let shapes = frame(&ctx, &mut tab, vec![], 0.1);
        let at = painted(&shapes, "production")
            .map(|t| t.visual_bounding_rect().center())
            .unwrap_or_default();
        frame(&ctx, &mut tab, vec![Event::PointerMoved(at)], 0.2);
        frame(&ctx, &mut tab, vec![], 0.9);
        let shapes = frame(&ctx, &mut tab, vec![], 1.5);
        let all: Vec<String> = texts(&shapes)
            .iter()
            .map(|t| t.galley.job.text.clone())
            .collect();
        let has = |needle: &str| all.iter().any(|s| s.contains(needle));
        assert!(has("Deployed to production"), "{all:?}");
        assert!(has("Source: GitHub Deployments API (gh)"), "{all:?}");
        assert!(has("Artifact site: sha256:feedface"), "{all:?}");
        assert!(has("Observed 2 minutes ago"), "{all:?}");
        assert!(
            has("a green build or a release tag is not a deployment"),
            "{all:?}"
        );
    })
}

#[test]
fn hidden_markers_are_not_drawn() {
    pinned(|| {
        let (ctx, mut tab) = fixture(false);
        frame(&ctx, &mut tab, vec![], 0.0);
        let shapes = frame(&ctx, &mut tab, vec![], 0.1);
        assert!(painted(&shapes, "Commit number 1").is_some());
        assert!(painted(&shapes, "production").is_none());
    })
}
