//! The start page's recent repositories: a search that filters them as one types, ↑ / ↓ and
//! Enter to open one, and for each its branch, uncommitted changes and when it was last opened.

mod row;
pub(super) use row::*;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Align2, Galley, Key, Modifiers, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::App;
use super::home_filter::{self, RecentFilter, filter, repo_name};
use super::recent_status::RecentStatus;
use crate::icons;
use crate::theme::{self, Tokens};

/// What was clicked in the list.
pub(super) enum RecentClick {
    Open(PathBuf),
    /// Remove the repository at this index of the recent list.
    Forget(usize),
}

/// What a row shows besides the repository's name and path.
struct RowInfo<'a> {
    status: Option<&'a RecentStatus>,
    /// "2 hours ago".
    opened: Option<String>,
    /// Picked with the keyboard.
    highlighted: bool,
}

/// The keys the search took this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SearchKeys {
    pub down: bool,
    pub up: bool,
    pub enter: bool,
    pub esc: bool,
}

/// The search field. While it has the keyboard it takes ↑ / ↓, Enter and Esc (which clears
/// it) before the field sees them; it takes the keyboard itself when `take_focus` and nothing
/// else has it.
pub(super) fn search_ui(ui: &mut Ui, f: &mut RecentFilter, take_focus: bool) -> SearchKeys {
    search_ui_at(ui, f, home_filter::search_id(), take_focus)
}

/// [`search_ui`] for the search field `id` (the start page has two: Recent's and the
/// library's), its hint the same.
pub(super) fn search_ui_at(
    ui: &mut Ui,
    f: &mut RecentFilter,
    id: egui::Id,
    take_focus: bool,
) -> SearchKeys {
    let mut keys = SearchKeys::default();
    if ui.memory(|m| m.has_focus(id)) {
        let has_text = !f.query.is_empty();
        keys = ui.input_mut(|i| SearchKeys {
            down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
            up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
            enter: i.consume_key(Modifiers::NONE, Key::Enter),
            esc: has_text && i.consume_key(Modifiers::NONE, Key::Escape),
        });
    }
    if take_focus && ui.memory(|m| m.focused().is_none()) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let before = f.query.clone();
    ui.scope(|ui| {
        // A blinking cursor would redraw the start page twice a second for nothing.
        ui.visuals_mut().text_cursor.blink = false;
        ui.add(
            egui::TextEdit::singleline(&mut f.query)
                .id(id)
                .hint_text(crate::i18n::text("interaction-home-filter"))
                .desired_width(200.0)
                // Esc stays with the field (it clears it), and so does the focus.
                .event_filter(egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                    tab: false,
                }),
        );
    });
    if f.query != before {
        f.selected = 0;
    }
    keys
}

/// Does what `keys` asked, with `matches` the rows the search leaves (as filtered after the
/// field took this frame's typing). Returns the index in the recent list to open (Enter).
pub(super) fn apply_keys(
    f: &mut RecentFilter,
    keys: SearchKeys,
    matches: &[usize],
) -> Option<usize> {
    if keys.esc {
        f.clear();
        return None;
    }
    f.clamp(matches.len());
    if keys.down {
        f.step(1, matches.len());
    }
    if keys.up {
        f.step(-1, matches.len());
    }
    if keys.enter {
        return f.picked(matches);
    }
    None
}

impl App {
    /// Whether a dialog of the window's has the keyboard (the start page's search then
    /// doesn't take it).
    fn dialog_open(&self) -> bool {
        self.clone_form.is_some()
            || self.remote_form.is_some()
            || self.askpass_open()
            || self.palette.is_some()
            || self.manual_open
            || self.repo_scan.is_some()
            || self.library_open
    }

    /// The recent repositories: the search, then the rows matching it.
    pub(super) fn recent_section(&mut self, ui: &mut Ui, t: &Tokens) -> Option<RecentClick> {
        let ctx = ui.ctx().clone();
        self.recent_status.update(&self.recent, &ctx);
        let take_focus = !self.dialog_open();
        let mut keys = SearchKeys::default();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(crate::i18n::text("interaction-home-recent"))
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                keys = search_ui(ui, &mut self.recent_filter, take_focus);
                icons::inline(ui, "search", 13.0, t.text_faint);
            });
        });
        let matches = filter(&self.recent, &self.recent_filter.query);
        let mut click = apply_keys(&mut self.recent_filter, keys, &matches)
            .map(|i| RecentClick::Open(self.recent[i].clone()));

        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        if matches.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-home-no-match",
                    &[("query", self.recent_filter.query.trim().into())],
                ))
                .color(t.text_faint),
            );
        }
        let searching = home_filter::search_has_focus(&ctx);
        let pointer_moved = ctx.input(|i| i.pointer.is_moving());
        let now = now_secs();
        for (row, &i) in matches.iter().enumerate() {
            let path = &self.recent[i];
            let info = RowInfo {
                status: self.recent_status.get(path),
                opened: self
                    .last_opened
                    .get(path)
                    .map(|&at| crate::git::relative_time(at, now)),
                highlighted: searching && row == self.recent_filter.selected,
            };
            let (clicked, hovered) = recent_row(ui, path, &info);
            // The mouse moves the highlight too: one row looks picked, never two.
            if hovered && pointer_moved {
                self.recent_filter.selected = row;
            }
            match clicked {
                Some(RowClick::Open) => click = Some(RecentClick::Open(path.clone())),
                Some(RowClick::Forget) => click = Some(RecentClick::Forget(i)),
                None => {}
            }
        }
        click
    }
}

/// The time now, in seconds since 1970.
pub(super) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "home_recent/localized_status_paint_tests.rs"]
mod localized_status_paint_tests;
#[cfg(test)]
#[path = "home_recent/tests.rs"]
mod tests;
