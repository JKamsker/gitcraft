//! The preview beside the settings: what the setting the pointer is on does, drawn as the app
//! would draw it with the values set now. The rows that have one say which (see
//! [`super::layout::Card::preview`]); the last one pointed at stays until another is.

mod app;
mod draw;
mod graph;
mod other;

use egui::{Id, RichText, Stroke, Ui, vec2};

use super::{Page, Settings};
use crate::theme::{self, ThemeId, Tokens};

pub(crate) use app::thumbnail;

/// What a preview shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Preview {
    /// The whole window in a theme: the one chosen, or the one pointed at in the gallery.
    Theme,
    Accent,
    Corners,
    CodeColors,
    Avatars,
    DiffLayout,
    DiffContext,
    FileList,
    GraphRefs,
    GraphColumns,
    GraphLanes,
    LaneColors,
    Pull,
    Terminal,
}

impl Preview {
    #[cfg(test)]
    pub(super) const ALL: [Preview; 14] = [
        Preview::Theme,
        Preview::Accent,
        Preview::Corners,
        Preview::CodeColors,
        Preview::Avatars,
        Preview::DiffLayout,
        Preview::DiffContext,
        Preview::FileList,
        Preview::GraphRefs,
        Preview::GraphColumns,
        Preview::GraphLanes,
        Preview::LaneColors,
        Preview::Pull,
        Preview::Terminal,
    ];

    fn title(self) -> &'static str {
        super::labels::display(match self {
            Preview::Theme => "Theme",
            Preview::Accent => "Accent colour",
            Preview::Corners => "Corners",
            Preview::CodeColors => "Code colours",
            Preview::Avatars => "Avatars",
            Preview::DiffLayout => "Diff layout",
            Preview::DiffContext => "Diff context",
            Preview::FileList => "Changed files",
            Preview::GraphRefs => "Branch column width",
            Preview::GraphColumns => "Columns",
            Preview::GraphLanes => "Lanes",
            Preview::LaneColors => "Lane colours",
            Preview::Pull => "Pull",
            Preview::Terminal => "Terminal",
        })
    }

    fn explain(self, s: &Settings) -> String {
        match self {
            Preview::Theme => crate::i18n::text("interaction-preview-theme").to_owned(),
            Preview::Accent => crate::i18n::text("interaction-preview-accent").to_owned(),
            Preview::Corners => crate::i18n::text("interaction-preview-corners").to_owned(),
            Preview::CodeColors => crate::i18n::text("interaction-preview-code").to_owned(),
            Preview::Avatars => crate::i18n::text("interaction-preview-avatars").to_owned(),
            Preview::DiffLayout => crate::i18n::text("interaction-preview-diff-layout").to_owned(),
            Preview::DiffContext => crate::i18n::text("interaction-preview-context").to_owned(),
            Preview::FileList => crate::i18n::text("interaction-preview-files").to_owned(),
            Preview::GraphRefs => crate::i18n::message(
                "interaction-preview-refs",
                &[
                    ("width", s.refs_width.round().into()),
                    (
                        "part",
                        if s.branch_names_keep_end {
                            "end"
                        } else {
                            "start"
                        }
                        .into(),
                    ),
                ],
            ),
            Preview::GraphColumns => crate::i18n::text("interaction-preview-columns").to_owned(),
            Preview::GraphLanes => crate::i18n::text("interaction-preview-lanes").to_owned(),
            Preview::LaneColors => crate::i18n::text("interaction-preview-lane-colours").to_owned(),
            Preview::Pull => match s.pull_mode {
                crate::repo::pull::PullMode::Merge => {
                    crate::i18n::text("interaction-preview-pull-merge").to_owned()
                }
                crate::repo::pull::PullMode::FastForwardOnly => {
                    crate::i18n::text("interaction-preview-pull-fastforwardonly").to_owned()
                }
                crate::repo::pull::PullMode::Rebase => {
                    crate::i18n::text("interaction-preview-pull-rebase").to_owned()
                }
            },
            Preview::Terminal => crate::i18n::text("interaction-preview-terminal").to_owned(),
        }
    }

    /// The preview a page shows before any row is pointed at.
    pub(super) fn of_page(page: Page) -> Option<Preview> {
        match page {
            Page::Appearance => Some(Preview::Theme),
            Page::Graph => Some(Preview::GraphRefs),
            Page::Git => Some(Preview::Pull),
            Page::Terminal => Some(Preview::Terminal),
            _ => None,
        }
    }
}

/// The preview a row asked for when it was last pointed at, and on which page.
#[derive(Clone, Copy)]
struct Pointed {
    page: Option<Page>,
    preview: Preview,
    /// Which row, of those with this preview.
    row: Id,
}

fn pointed_id() -> Id {
    Id::new("settings-preview")
}

/// A row with `preview` is pointed at (on `page`, or in the search results when `None`).
pub(super) fn point(ctx: &egui::Context, page: Option<Page>, preview: Preview, row: Id) {
    let before = ctx.data(|d| d.get_temp::<Pointed>(pointed_id()));
    if before.is_none_or(|b| b.preview != preview || b.page != page || b.row != row) {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(pointed_id(), Pointed { page, preview, row }));
}

/// Forgets the row pointed at last: what a search found changed.
pub(super) fn forget(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Pointed>(pointed_id()));
}

fn first_found_id() -> Id {
    Id::new("settings-preview-first-found")
}

/// A row with `preview` is drawn among the search results: the first one drawn in a frame
/// shows its preview until another is pointed at.
pub(super) fn found(ctx: &egui::Context, preview: Preview) {
    let frame = ctx.cumulative_frame_nr();
    let first = ctx.data(|d| d.get_temp::<(u64, Preview)>(first_found_id()));
    if first.is_none_or(|(at, _)| at != frame) {
        ctx.data_mut(|d| d.insert_temp(first_found_id(), (frame, preview)));
    }
}

/// The preview to show for `page` (`None`: the search results).
pub(super) fn shown(ctx: &egui::Context, page: Option<Page>) -> Option<Preview> {
    let pointed = ctx.data(|d| d.get_temp::<Pointed>(pointed_id()));
    match pointed {
        Some(p) if p.page == page => Some(p.preview),
        _ => match page {
            Some(page) => Preview::of_page(page),
            None => {
                // The first found the frame before, if there was one then.
                let frame = ctx.cumulative_frame_nr();
                ctx.data(|d| d.get_temp::<(u64, Preview)>(first_found_id()))
                    .filter(|(at, _)| frame.saturating_sub(*at) <= 1)
                    .map(|(_, p)| p)
            }
        },
    }
}

/// Whether `preview` is what shows now: its row is marked.
/// Whether the row `row` with `preview` is the one whose preview shows: the one pointed at,
/// or else the first drawn this frame with the preview shown.
pub(super) fn is_marked(
    ctx: &egui::Context,
    page: Option<Page>,
    preview: Preview,
    row: Id,
) -> bool {
    if shown(ctx, page) != Some(preview) {
        return false;
    }
    let pointed = ctx.data(|d| d.get_temp::<Pointed>(pointed_id()));
    if let Some(p) = pointed.filter(|p| p.page == page) {
        return p.row == row;
    }
    let frame = ctx.cumulative_frame_nr();
    let id = Id::new("settings-preview-marked");
    let marked = ctx.data(|d| d.get_temp::<u64>(id)) == Some(frame);
    if !marked {
        ctx.data_mut(|d| d.insert_temp(id, frame));
    }
    !marked
}

fn theme_hover_id() -> Id {
    Id::new("settings-preview-theme")
}

/// The theme pointed at in the gallery this frame, shown in the preview in place of the one
/// chosen.
pub(super) fn point_theme(ctx: &egui::Context, id: &ThemeId) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| d.insert_temp(theme_hover_id(), (frame, id.clone())));
}

/// The theme pointed at in the gallery in this frame or the last one.
fn pointed_theme(ctx: &egui::Context) -> Option<ThemeId> {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|d| d.get_temp::<(u64, ThemeId)>(theme_hover_id()))
        .filter(|(at, _)| frame.saturating_sub(*at) <= 1)
        .map(|(_, id)| id)
}

/// The preview panel: the setting's name, what it does, and the drawing.
pub(super) fn panel(ui: &mut Ui, preview: Preview, settings: &Settings, current: &ThemeId) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.separator))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(crate::i18n::text("interaction-preview-heading"))
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
            ui.add_space(2.0);
            // A theme pointed at in the gallery shows in its own colours.
            let hovered = matches!(preview, Preview::Theme | Preview::Accent | Preview::Corners)
                .then(|| pointed_theme(ui.ctx()))
                .flatten();
            let shown_theme = hovered.as_ref().unwrap_or(current);
            let title = if preview == Preview::Theme {
                shown_theme.label()
            } else {
                preview.title().to_owned()
            };
            ui.label(RichText::new(title).font(theme::semibold(15.0)));
            if preview == Preview::Theme
                && let Some(info) = theme::find(shown_theme)
            {
                ui.label(RichText::new(&info.credit).color(t.text_faint).small());
            }
            ui.add_space(4.0);
            ui.label(RichText::new(preview.explain(settings)).color(t.text_dim));
            ui.add_space(12.0);
            let width = ui.available_width();
            match preview {
                Preview::Theme | Preview::Accent | Preview::Corners => {
                    let tokens = match &hovered {
                        Some(id) => theme::resolve(id).tokens(&settings.theme_tweaks),
                        None => t,
                    };
                    if preview == Preview::Corners {
                        app::corners(ui, &tokens, width);
                    }
                    app::window(ui, &tokens, width, preview);
                }
                Preview::GraphRefs => graph::refs(ui, &t, settings, width),
                Preview::GraphColumns => graph::columns(ui, &t, settings, width),
                Preview::GraphLanes => graph::lanes(ui, &t, settings, width),
                Preview::LaneColors => graph::lane_colors(ui, &t, settings, width, current),
                Preview::Pull => other::pull(ui, &t, settings, width),
                Preview::Terminal => other::terminal(ui, &t, settings, width),
                Preview::CodeColors => other::code(ui, &t, settings, width),
                Preview::Avatars => other::avatars(ui, &t, settings, width),
                Preview::DiffLayout => other::diff_layout(ui, &t, settings, width),
                Preview::DiffContext => other::diff_context(ui, &t, settings, width),
                Preview::FileList => other::file_list(ui, &t, settings, width),
            }
        });
}

/// Room the preview panel takes beside the page, from what is left of the window: none when
/// the page would get too narrow (the preview then goes above it).
pub(super) fn side_width(room: f32) -> Option<f32> {
    const MIN_PAGE: f32 = 520.0;
    const GAP: f32 = 28.0;
    let width = (room - MIN_PAGE - GAP).clamp(0.0, 440.0);
    (width >= 330.0).then_some(width)
}

/// Room between the page and the preview beside it.
pub(super) const SIDE_GAP: f32 = 28.0;
/// Room right of the preview, to the window's edge.
pub(super) const SIDE_MARGIN: f32 = 24.0;

/// The preview above the page, in a narrow window: no taller than this.
pub(super) const ABOVE_MAX_H: f32 = 300.0;

/// Size of the drawing area of a preview `width` wide and `height` tall.
fn area(ui: &mut Ui, width: f32, height: f32) -> egui::Rect {
    ui.allocate_exact_size(vec2(width, height), egui::Sense::hover())
        .0
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    fn texts(shape: &egui::Shape, found: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => found.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, found);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn actual_inert_previews_paint_german_and_keep_sample_refs_commands_and_settings() {
        crate::i18n::set_language("de");
        let settings = Settings {
            refs_width: 2000.0,
            vscode_colors: false,
            show_date: true,
            show_when: true,
            date_width: 250.0,
            branch_names_keep_end: true,
            ..Settings::default()
        };
        let saved = serde_json::to_value(&settings).unwrap();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::theme::install_fonts(ui.ctx());
        })
        .drop_without_applying_deltas();
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1500.0, 7000.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.set_width(1400.0);
                for preview in [
                    Preview::GraphRefs,
                    Preview::GraphColumns,
                    Preview::GraphLanes,
                    Preview::Terminal,
                    Preview::Pull,
                    Preview::CodeColors,
                    Preview::Avatars,
                    Preview::DiffLayout,
                    Preview::DiffContext,
                    Preview::FileList,
                ] {
                    panel(ui, preview, &settings, &ThemeId::new("pro"));
                }
            },
        );
        let mut found = Vec::new();
        for shape in &out.shapes {
            texts(&shape.shape, &mut found);
        }
        out.drop_without_applying_deltas();
        for expected in [
            "VORSCHAU",
            "BESCHREIBUNG",
            "Einstellungen mit Vorschau daneben",
            "vor 5 Minuten",
            "06.11.2026 um 14:02",
            "git status --short",
            "feature/settings-previews",
            "Nebeneinander",
            "In Ordnern",
        ] {
            assert!(
                found.iter().any(|text| text.contains(expected)),
                "missing {expected}: {found:?}"
            );
        }
        assert!(Preview::GraphRefs.explain(&settings).contains("2.000 px"));
        assert!(
            Preview::GraphRefs
                .explain(&settings)
                .contains("ihr Ende (…xyz)")
        );
        assert_eq!(serde_json::to_value(&settings).unwrap(), saved);
        crate::i18n::set_language("en-US");
        assert!(Preview::GraphRefs.explain(&settings).contains("2,000 px"));
        assert_eq!(Preview::DiffLayout.title(), "Diff layout");
    }
}
