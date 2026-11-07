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
        match self {
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
        }
    }

    fn explain(self, s: &Settings) -> String {
        match self {
            Preview::Theme => "The colours of the whole window: tabs, panels, the graph, diffs \
                               and the terminal. Point at a theme to see it here first."
                .to_owned(),
            Preview::Accent => "Buttons, selections, focused fields and links. Your own \
                                colour, or the one the theme comes with."
                .to_owned(),
            Preview::Corners => "How round buttons, fields, pills and panels are.".to_owned(),
            Preview::CodeColors => {
                "Code in diffs and blame, coloured as your editor colours it.".to_owned()
            }
            Preview::Avatars => {
                "Beside each commit's author in the graph and the details.".to_owned()
            }
            Preview::DiffLayout => "How a file's changes show: old and new in one column, or \
                                    beside each other."
                .to_owned(),
            Preview::DiffContext => "Only the lines around each change, or the whole file with \
                                     the changes in place."
                .to_owned(),
            Preview::FileList => "The changed files of a commit or the working tree.".to_owned(),
            Preview::GraphRefs => format!(
                "The room left of the graph for the branch and tag pills: {:.0} px. Names \
                 longer than fits keep their {}; drag the column's edge in the \
                 graph's header to change it there.",
                s.refs_width,
                if s.branch_names_keep_end {
                    "end (…xyz)"
                } else {
                    "start (xyz…)"
                }
            ),
            Preview::GraphColumns => "The columns right of each commit's description. Their \
                                      widths are dragged in the graph's header."
                .to_owned(),
            Preview::GraphLanes => "How far apart the graph's lanes are. Compact lanes keep a \
                                    busy history's graph narrow."
                .to_owned(),
            Preview::LaneColors => {
                "The colour of each lane of the graph, and of its pills.".to_owned()
            }
            Preview::Pull => format!(
                "What the toolbar's Pull does when your branch and its remote have both moved \
                 on: {}.",
                s.pull_mode.label().to_lowercase()
            ),
            Preview::Terminal => "The integrated terminal, with the theme's colours.".to_owned(),
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
                RichText::new("PREVIEW")
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
