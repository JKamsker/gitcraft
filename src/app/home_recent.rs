//! The start page's recent repositories: a search that filters them as one types, ↑ / ↓ and
//! Enter to open one, and for each its branch, uncommitted changes and when it was last opened.

use std::path::{Path, PathBuf};

use egui::{Align2, Key, Modifiers, Rect, RichText, Sense, Ui, pos2, vec2};

use super::App;
use super::home_filter::{self, filter, repo_name};
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
    highlighted: bool,
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
    }

    /// The recent repositories: the search, then the rows matching it.
    pub(super) fn recent_section(&mut self, ui: &mut Ui, t: &Tokens) -> Option<RecentClick> {
        let ctx = ui.ctx().clone();
        self.recent_status.update(&self.recent, &ctx);
        let matches = filter(&self.recent, &self.recent_filter.query);
        self.recent_filter.clamp(matches.len());
        let mut click = self.recent_keys(&ctx, &matches);

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("RECENT")
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.recent_search(ui, &ctx);
            });
        });
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        if matches.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "No recent repository matches “{}”",
                    self.recent_filter.query.trim()
                ))
                .color(t.text_faint),
            );
        }
        let now = now_secs();
        for (row, &i) in matches.iter().enumerate() {
            let path = &self.recent[i];
            let info = RowInfo {
                status: self.recent_status.get(path),
                opened: self
                    .last_opened
                    .get(path)
                    .map(|&at| crate::git::relative_time(at, now)),
                highlighted: row == self.recent_filter.selected
                    && home_filter::search_has_focus(&ctx),
            };
            match recent_row(ui, path, &info) {
                Some(RowClick::Open) => click = Some(RecentClick::Open(path.clone())),
                Some(RowClick::Forget) => click = Some(RecentClick::Forget(i)),
                None => {}
            }
        }
        click
    }

    /// The search field, which has the keyboard whenever nothing else does.
    fn recent_search(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let id = home_filter::search_id();
        let nothing_focused = ctx.memory(|m| m.focused().is_none());
        if nothing_focused && !self.dialog_open() {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let before = self.recent_filter.query.clone();
        ui.add(
            egui::TextEdit::singleline(&mut self.recent_filter.query)
                .id(id)
                .hint_text("Type to filter")
                .desired_width(200.0),
        );
        if self.recent_filter.query != before {
            self.recent_filter.selected = 0;
        }
        icons::inline(ui, "search", 13.0, Tokens::get(ctx).text_faint);
    }

    /// ↑ / ↓ move the highlight, Enter opens the highlighted repository, Esc clears the search,
    /// while the search has the keyboard.
    fn recent_keys(&mut self, ctx: &egui::Context, matches: &[usize]) -> Option<RecentClick> {
        if !home_filter::search_has_focus(ctx) || self.dialog_open() {
            return None;
        }
        let (down, up, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                !self.recent_filter.query.is_empty() && i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if down {
            self.recent_filter.step(1, matches.len());
        }
        if up {
            self.recent_filter.step(-1, matches.len());
        }
        if esc {
            self.recent_filter.clear();
        }
        if enter {
            let picked = self.recent_filter.picked(matches)?;
            return Some(RecentClick::Open(self.recent[picked].clone()));
        }
        None
    }
}

/// The time now, in seconds since 1970.
pub(super) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

enum RowClick {
    Open,
    Forget,
}

fn recent_row(ui: &mut Ui, path: &Path, info: &RowInfo) -> Option<RowClick> {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let hovered = resp.hovered();
    if hovered || info.highlighted {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    let y = rect.center().y;
    // One on another machine says which.
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 20.0, y), vec2(18.0, 18.0)),
        if crate::remote::is_remote(path) {
            "server"
        } else {
            "folder-git-2"
        },
        16.0,
        t.icon,
    );
    // Leave room for the remove button; long paths are cut at the row's edge.
    let right = rect.right() - 36.0;
    let p = ui.painter_at(rect.with_max_x(right));
    let opened_left = info.opened.as_ref().map_or(right, |opened| {
        p.text(
            pos2(right, y - 8.0),
            Align2::RIGHT_CENTER,
            opened,
            egui::FontId::proportional(11.0),
            t.text_faint,
        )
        .left()
            - 12.0
    });
    let name = p.text(
        pos2(rect.left() + 40.0, y - 8.0),
        Align2::LEFT_CENTER,
        repo_name(path),
        theme::medium(12.5),
        t.text,
    );
    if let Some(status) = info.status {
        let at = pos2(name.right() + 10.0, y - 8.0);
        status_text(ui, &p, status, path, at, opened_left);
    }
    p.text(
        pos2(rect.left() + 40.0, y + 8.0),
        Align2::LEFT_CENTER,
        crate::remote::display(path),
        egui::FontId::proportional(11.0),
        t.text_faint,
    );
    let mut forget = false;
    if hovered || ui.rect_contains_pointer(rect) {
        let x_rect = Rect::from_center_size(pos2(rect.right() - 18.0, y), vec2(20.0, 20.0));
        let x = ui
            .interact(x_rect, resp.id.with("forget"), Sense::click())
            .on_hover_text("Remove from list");
        if x.hovered() {
            ui.painter().rect_filled(x_rect, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x_rect, "x", 12.0, t.text_faint);
        forget = x.clicked();
    }
    if forget {
        Some(RowClick::Forget)
    } else if resp.clicked() {
        Some(RowClick::Open)
    } else {
        None
    }
}

/// The branch (or what is wrong) after the name, starting at `at` and cut before `max_x`, and
/// a dot after it when there are uncommitted changes.
fn status_text(
    ui: &Ui,
    p: &egui::Painter,
    status: &RecentStatus,
    path: &Path,
    at: egui::Pos2,
    max_x: f32,
) {
    let t = Tokens::get(ui.ctx());
    let label = status.label();
    let (color, icon) = if status.is_problem() {
        (t.warning, "circle-alert")
    } else {
        (t.text_dim, "git-branch")
    };
    if label.is_empty() || at.x + 16.0 > max_x {
        return;
    }
    let icon_rect = Rect::from_center_size(pos2(at.x + 6.0, at.y), vec2(12.0, 12.0));
    if p.clip_rect().contains_rect(icon_rect) {
        icons::paint(ui, icon_rect, icon, 11.0, color);
    }
    let clip = p.with_clip_rect(
        p.clip_rect()
            .intersect(Rect::from_x_y_ranges(at.x..=max_x, p.clip_rect().y_range())),
    );
    let text = clip.text(
        pos2(at.x + 15.0, at.y),
        Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.0),
        color,
    );
    if status.dirty() && text.right() + 10.0 < max_x {
        let dot = pos2(text.right() + 7.0, at.y);
        clip.circle_filled(dot, 3.5, t.warning);
        let hit = Rect::from_center_size(dot, vec2(10.0, 10.0));
        ui.interact(hit, egui::Id::new(("recent-dirty", path)), Sense::hover())
            .on_hover_text("Uncommitted changes");
    }
}
