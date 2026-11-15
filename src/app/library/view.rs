//! The start page's _All repositories_ section: a filter (All, Recent, Nearby, each root), a
//! search with the Recent list's keys (↑ / ↓, Enter, Esc), and the rows, laid out only as
//! far as they are scrolled into view.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};

use super::model::{Library, Source};
use super::row::{RowClick, library_row};
use super::rows::{self, Row, Show};
use super::status;
use crate::app::App;
use crate::app::home_filter::RecentFilter;
use crate::app::home_recent::{SearchKeys, apply_keys, search_ui_at};
use crate::app::recent_status::RecentStatuses;
use crate::icons;
use crate::theme::{self, Tokens};

/// A row's height, with the space between rows.
pub(in crate::app) const ROW_H: f32 = 42.0;
/// How many rows are in view at most before the list scrolls.
const ROWS_IN_VIEW: f32 = 8.0;

/// The id of the library's search field.
pub(in crate::app) fn search_id() -> egui::Id {
    egui::Id::new("home-library-filter")
}

/// What the section shows.
#[derive(Default)]
pub(in crate::app) struct LibraryView {
    pub filter: RecentFilter,
    pub show: Show,
    /// How far the rows are scrolled, and how tall their view is (last frame).
    offset: f32,
    height: f32,
}

/// The rows made for the library as it last was: made again only when the profile, the
/// library or Recent changed, not every frame.
#[derive(Default)]
pub(in crate::app) struct RowsCache {
    key: Option<(u64, u64, Vec<PathBuf>)>,
    pub(super) rows: Vec<Row>,
}

impl RowsCache {
    fn get(&mut self, profile: u64, rev: u64, lib: &Library, recent: &[PathBuf]) -> &[Row] {
        let fresh = self
            .key
            .as_ref()
            .is_some_and(|(p, r, rec)| *p == profile && *r == rev && rec == recent);
        if !fresh {
            self.rows = rows::all(lib, recent);
            self.key = Some((profile, rev, recent.to_vec()));
        }
        &self.rows
    }
}

/// What was asked of the section.
pub(in crate::app) enum LibraryClick {
    Open(PathBuf),
    Hide(PathBuf),
    Manage,
    AddRoot,
}

impl LibraryView {
    /// The scroll offset that brings row `row` into view, when it isn't.
    fn follow(&self, row: usize) -> Option<f32> {
        let top = row as f32 * ROW_H;
        if top < self.offset {
            Some(top)
        } else if top + ROW_H > self.offset + self.height {
            Some(top + ROW_H - self.height)
        } else {
            None
        }
    }
}

impl App {
    /// The section, under Recent. Returns what was asked.
    pub(in crate::app) fn library_section(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
    ) -> Option<LibraryClick> {
        let profile = self.settings.active_profile;
        let state = &mut self.library;
        let lib = state.saved.entry(profile).or_default();
        let view = &mut state.view;
        let mut click = None;
        let mut keys = SearchKeys::default();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(crate::i18n::text("interaction-library-title"))
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .link(crate::i18n::text("interaction-library-manage"))
                    .clicked()
                {
                    click = Some(LibraryClick::Manage);
                }
                ui.add_space(8.0);
                keys = search_ui_at(ui, &mut view.filter, search_id(), false);
                icons::inline(ui, "search", 13.0, t.text_faint);
            });
        });
        show_chips(ui, view, lib);
        let rows = state.rows.get(profile, state.rev, lib, &self.recent);
        let matches = rows::matching(rows, &view.show, &view.filter.query);
        let before = view.filter.selected;
        if let Some(i) = apply_keys(&mut view.filter, keys, &matches) {
            click = Some(LibraryClick::Open(rows[i].path.clone()));
        }
        let moved = view.filter.selected != before || keys.down || keys.up;
        let follow = moved.then(|| view.follow(view.filter.selected)).flatten();
        status::source_line(ui, t, view, lib, &state.scheduler, profile);
        if rows.is_empty() && lib.roots.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::i18n::text("interaction-library-empty")).color(t.text_faint),
            );
            if ui
                .button(crate::i18n::text("interaction-library-add-root"))
                .clicked()
            {
                click = Some(LibraryClick::AddRoot);
            }
            return click;
        }
        if matches.is_empty() {
            ui.add_space(6.0);
            let text = if view.filter.query.trim().is_empty() {
                crate::i18n::text("interaction-library-none-here").to_owned()
            } else {
                crate::i18n::message(
                    "interaction-home-no-match",
                    &[("query", view.filter.query.trim().into())],
                )
            };
            ui.label(RichText::new(text).color(t.text_faint));
            return click;
        }
        let shown = Shown {
            rows,
            matches: &matches,
            lib,
            statuses: &self.recent_status,
        };
        rows_ui(ui, view, &shown, follow).or(click)
    }
}

/// The rows to lay out, and what their tags and statuses come from.
struct Shown<'a> {
    rows: &'a [Row],
    matches: &'a [usize],
    lib: &'a Library,
    statuses: &'a RecentStatuses,
}

/// The rows in view, virtualized: only those scrolled to are laid out. A row's branch and
/// changes show only for a Recent one (already read for the Recent list): nothing runs git
/// for a repository found by a scan until it is opened.
fn rows_ui(
    ui: &mut Ui,
    view: &mut LibraryView,
    shown: &Shown,
    follow: Option<f32>,
) -> Option<LibraryClick> {
    let ctx = ui.ctx().clone();
    let searching = ctx.memory(|m| m.has_focus(search_id()));
    let pointer_moved = ctx.input(|i| i.pointer.is_moving());
    let mut click = None;
    let mut area = egui::ScrollArea::vertical()
        .id_salt("library-rows")
        .max_height(ROW_H * ROWS_IN_VIEW)
        .auto_shrink([false, true]);
    if let Some(y) = follow {
        area = area.vertical_scroll_offset(y);
    }
    let selected = view.filter.selected;
    let mut hovered_row = None;
    let out = area.show_rows(ui, ROW_H - 2.0, shown.matches.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for at in range {
            let row = &shown.rows[shown.matches[at]];
            let tag = source_tag(row, shown.lib);
            let status = if row.recent {
                shown.statuses.get(&row.path)
            } else {
                None
            };
            let (clicked, hovered) =
                library_row(ui, row, &tag, status, searching && at == selected);
            if hovered && pointer_moved {
                hovered_row = Some(at);
            }
            match clicked {
                Some(RowClick::Open) => click = Some(LibraryClick::Open(row.path.clone())),
                Some(RowClick::Hide) => click = Some(LibraryClick::Hide(row.path.clone())),
                None => {}
            }
        }
    });
    if let Some(at) = hovered_row {
        view.filter.selected = at;
    }
    view.offset = out.state.offset.y;
    view.height = out.inner_rect.height();
    click
}

/// All, Recent, Nearby and each enabled root, to show only those.
fn show_chips(ui: &mut Ui, view: &mut LibraryView, lib: &Library) {
    use crate::i18n::text;
    let mut chips = vec![
        (Show::All, text("interaction-library-all").to_owned()),
        (Show::Recent, text("interaction-library-recent").to_owned()),
    ];
    if lib.nearby_enabled && !lib.nearby.is_empty() {
        chips.push((Show::Nearby, text("interaction-library-nearby").to_owned()));
    }
    for root in lib.roots.iter().filter(|r| r.enabled) {
        chips.push((Show::Root(root.id), root_name(&root.path)));
    }
    if !chips.iter().any(|(s, _)| *s == view.show) {
        view.show = Show::All;
    }
    ui.horizontal_wrapped(|ui| {
        for (show, label) in chips {
            if ui.selectable_label(view.show == show, label).clicked() {
                view.show = show;
                view.filter.selected = 0;
            }
        }
    });
}

/// A root's name in the filter: its folder's name.
pub(in crate::app) fn root_name(path: &Path) -> String {
    crate::app::home_filter::repo_name(path)
}

/// Where a row's repository was found, in words: Recent, the roots' names, Nearby; Stale
/// when no scan saw it lately.
pub(in crate::app) fn source_tag(row: &Row, lib: &Library) -> String {
    use crate::i18n::text;
    let mut parts = Vec::new();
    if row.recent {
        parts.push(text("interaction-library-recent").to_owned());
    }
    for source in &row.sources {
        match source {
            Source::Root(id) => parts.extend(lib.root(*id).map(|r| root_name(&r.path))),
            Source::Nearby(_) => {
                let nearby = text("interaction-library-nearby").to_owned();
                if !parts.contains(&nearby) {
                    parts.push(nearby);
                }
            }
        }
    }
    if row.stale {
        parts.push(text("interaction-library-stale").to_owned());
    }
    parts.join(" · ")
}
