//! The insights page in a tab: opening it over what the big area showed, going from view to
//! view, Esc, and every view drawing without a repository to read.

use super::{InsightsAct, Kind};
use crate::repo::{Action, RepoTab};

/// One frame of the page; what it asks for is carried out.
pub(super) fn draw(tab: &mut RepoTab, ctx: &egui::Context, width: f32) {
    let mut acts = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 760.0),
        )),
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.insights_ui(ui, &mut acts));
    })
    .drop_without_applying_deltas();
    for act in acts {
        tab.apply(act, ctx);
    }
}

#[test]
fn the_page_opens_over_the_big_area_and_goes_from_view_to_view() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-page".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Evolution)), &ctx);
    assert_eq!(tab.insights.as_ref().map(|v| v.kind), Some(Kind::Evolution));
    for kind in Kind::ALL {
        tab.apply(Action::Insights(InsightsAct::Open(kind)), &ctx);
        assert_eq!(tab.insights.as_ref().map(|v| v.kind), Some(kind));
        // Wide (the list of views beside it) and narrow (a menu above it).
        draw(&mut tab, &ctx, 1200.0);
        draw(&mut tab, &ctx, 600.0);
        tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
        draw(&mut tab, &ctx, 1200.0);
        tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    }
    tab.apply(Action::Insights(InsightsAct::Close), &ctx);
    assert!(tab.insights.is_none());
}

#[test]
fn esc_closes_the_page_once_nothing_in_the_view_is_selected() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-esc".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Coupling)), &ctx);
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
}

#[test]
fn every_view_is_in_the_palette_and_listed_once() {
    let ctx = egui::Context::default();
    let tab = RepoTab::new_deferred("/nonexistent/insights-palette".into(), &ctx);
    let items = tab.insights_palette_items();
    assert_eq!(items.len(), Kind::ALL.len());
    let mut titles: Vec<&str> = Kind::ALL.iter().map(|k| k.title()).collect();
    titles.sort();
    titles.dedup();
    assert_eq!(titles.len(), Kind::ALL.len());
    for kind in Kind::ALL {
        assert!(!kind.blurb().is_empty());
        assert!(crate::icons::known(kind.icon()), "{:?}", kind);
    }
}

#[test]
fn insights_replaces_investigation_and_document_pages() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/insights-navigation".into(), &ctx);
    // Keep this transition test inert: all requested insight data is already loaded.
    let mut view = super::InsightsView::new(Kind::Evolution);
    view.loaded.insert(Kind::Evolution);
    tab.insights = Some(Box::new(view));
    tab.symbols.open = true;
    tab.dossier.open = true;
    tab.review_document.open = true;
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Evolution)), &ctx);
    assert!(!tab.symbols.open);
    assert!(!tab.dossier.open);
    assert!(!tab.review_document.open);
    assert_eq!(
        tab.insights.as_ref().map(|view| view.kind),
        Some(Kind::Evolution)
    );
}
