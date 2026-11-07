//! The start page's recent repositories: a search that filters them as one types, ↑ / ↓ and
//! Enter to open one, and for each its branch, uncommitted changes and when it was last opened.

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
    let id = home_filter::search_id();
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
                .hint_text("Type to filter")
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
    }

    /// The recent repositories: the search, then the rows matching it.
    pub(super) fn recent_section(&mut self, ui: &mut Ui, t: &Tokens) -> Option<RecentClick> {
        let ctx = ui.ctx().clone();
        self.recent_status.update(&self.recent, &ctx);
        let take_focus = !self.dialog_open();
        let mut keys = SearchKeys::default();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("RECENT")
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
                RichText::new(format!(
                    "No recent repository matches “{}”",
                    self.recent_filter.query.trim()
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

enum RowClick {
    Open,
    Forget,
}

/// The room left between a name and the time it was opened, `room` wide, shared: the name
/// (`name` wide), the dot of uncommitted changes and the branch (`status` wide) all as they
/// are when they fit; else the branch keeps at least about a third, the name the rest. Returns
/// the widths the name and the branch may take.
pub(super) fn share_room(room: f32, name: f32, dot: f32, status: f32) -> (f32, f32) {
    const GAP: f32 = 10.0;
    let room = room.max(0.0);
    let want = name + dot + GAP + status;
    if want <= room {
        return (name, status);
    }
    let status_min = (room * 0.35).min(status);
    let name_max = (room - dot - GAP - status_min).max(0.0).min(name);
    let status_max = (room - dot - GAP - name_max).clamp(0.0, status);
    (name_max, status_max)
}

/// `text` on one line, cut with "…" to fit `max` wide.
fn elided(
    ui: &Ui,
    text: String,
    font: egui::FontId,
    color: egui::Color32,
    max: f32,
) -> Arc<Galley> {
    let mut job =
        egui::text::LayoutJob::single_section(text, egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: max.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts_mut(|f| f.layout_job(job))
}

/// A recent repository's row. Returns what was clicked, and whether the mouse is over it.
fn recent_row(ui: &mut Ui, path: &Path, info: &RowInfo) -> (Option<RowClick>, bool) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let hovered = resp.hovered();
    if hovered || info.highlighted {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    if info.highlighted {
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0));
        ui.painter().rect_filled(bar, 1.5, t.accent);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(
            rect,
            t.radius,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Inside,
        );
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
    // Leave room for the remove button.
    let left = rect.left() + 40.0;
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
    first_line(ui, &p, path, info.status, pos2(left, y - 8.0), opened_left);
    let shown = elided(
        ui,
        crate::remote::display(path),
        egui::FontId::proportional(11.0),
        t.text_faint,
        right - left,
    );
    p.galley(
        pos2(left, y + 8.0 - shown.size().y / 2.0),
        shown,
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
    let clicked = if forget {
        Some(RowClick::Forget)
    } else if resp.clicked() {
        Some(RowClick::Open)
    } else {
        None
    };
    (clicked, hovered)
}

/// The name, the dot of uncommitted changes, then the branch (or what is wrong), from `at` to
/// `max_x`: each cut with "…" rather than overlapping what follows.
fn first_line(
    ui: &Ui,
    p: &egui::Painter,
    path: &Path,
    status: Option<&RecentStatus>,
    at: egui::Pos2,
    max_x: f32,
) {
    let t = Tokens::get(ui.ctx());
    let name_font = theme::medium(12.5);
    let status_font = egui::FontId::proportional(11.0);
    let full_name = elided(
        ui,
        repo_name(path),
        name_font.clone(),
        t.text,
        f32::INFINITY,
    );
    let (label, color, icon) = match status {
        Some(s) if s.is_problem() => (s.label(), t.warning, "circle-alert"),
        Some(s) => (s.label(), t.text_dim, "git-branch"),
        None => (String::new(), t.text_dim, ""),
    };
    let dirty = status.is_some_and(RecentStatus::dirty);
    const ICON: f32 = 15.0;
    let full_status = elided(ui, label.clone(), status_font.clone(), color, f32::INFINITY);
    let status_w = if label.is_empty() {
        0.0
    } else {
        ICON + full_status.size().x
    };
    let dot_w = if dirty { 14.0 } else { 0.0 };
    let (name_w, status_room) = share_room(max_x - at.x, full_name.size().x, dot_w, status_w);
    let name = elided(ui, repo_name(path), name_font, t.text, name_w);
    let name_right = at.x + name.size().x;
    p.galley(pos2(at.x, at.y - name.size().y / 2.0), name, t.text);
    let mut x = name_right;
    if dirty {
        let dot = pos2(x + 8.0, at.y);
        p.circle_filled(dot, 3.5, t.warning);
        let hit = Rect::from_center_size(dot, vec2(12.0, 12.0));
        ui.interact(hit, egui::Id::new(("recent-dirty", path)), Sense::hover())
            .on_hover_text("Uncommitted changes");
        x += dot_w;
    }
    let Some(status) = status else { return };
    if label.is_empty() || status_room < ICON + 12.0 {
        return;
    }
    x += 10.0;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(x + 6.0, at.y), vec2(12.0, 12.0)),
        icon,
        11.0,
        color,
    );
    let text = elided(ui, label, status_font, color, status_room - ICON);
    let text_rect = Rect::from_min_size(pos2(x + ICON, at.y - text.size().y / 2.0), text.size());
    p.galley(text_rect.min, text, color);
    if let Some(why) = status.reason() {
        ui.interact(
            text_rect,
            egui::Id::new(("recent-why", path)),
            Sense::hover(),
        )
        .on_hover_text(why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_leaves_room_for_the_branch() {
        // All fits.
        assert_eq!(share_room(400.0, 100.0, 14.0, 80.0), (100.0, 80.0));
        // A very long name: the branch keeps about a third, the dot its room.
        let (name, status) = share_room(300.0, 900.0, 14.0, 200.0);
        assert!((status - 105.0).abs() < 0.1, "{status}");
        assert!(name + 14.0 + 10.0 + status <= 300.0 + 0.1);
        // A short branch keeps all it needs.
        let (name, status) = share_room(300.0, 900.0, 0.0, 40.0);
        assert_eq!(status, 40.0);
        assert!((name - 250.0).abs() < 0.1);
        // No room at all.
        assert_eq!(share_room(-5.0, 50.0, 14.0, 50.0), (0.0, 0.0));
    }

    #[test]
    fn keys_act_on_what_the_search_leaves() {
        let mut f = RecentFilter::default();
        let down = SearchKeys {
            down: true,
            ..Default::default()
        };
        let enter = SearchKeys {
            enter: true,
            ..Default::default()
        };
        assert_eq!(apply_keys(&mut f, down, &[3, 5]), None);
        assert_eq!(apply_keys(&mut f, enter, &[3, 5]), Some(5));
        // The list shrank under the highlight (typed in the same frame as Enter).
        assert_eq!(apply_keys(&mut f, enter, &[7]), Some(7));
        assert_eq!(apply_keys(&mut f, enter, &[]), None);
        f.query = "x".into();
        let esc = SearchKeys {
            esc: true,
            ..Default::default()
        };
        assert_eq!(apply_keys(&mut f, esc, &[1]), None);
        assert!(f.query.is_empty());
    }

    /// Driven like the window: typing filters, Esc clears it (and keeps the keyboard), ↓ and
    /// Enter open the picked one, also when typed in the same frame as Enter.
    #[test]
    fn the_search_is_driven_by_the_keyboard() {
        use egui::Event;
        struct State {
            filter: RecentFilter,
            recent: Vec<PathBuf>,
            opened: Option<PathBuf>,
        }
        let recent: Vec<PathBuf> = ["/x/alpha", "/x/beta", "/x/bravo"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let state = State {
            filter: RecentFilter::default(),
            recent,
            opened: None,
        };
        let mut harness = egui_kittest::Harness::builder().build_ui_state(
            |ui, s: &mut State| {
                let keys = search_ui(ui, &mut s.filter, true);
                let matches = filter(&s.recent, &s.filter.query);
                if let Some(i) = apply_keys(&mut s.filter, keys, &matches) {
                    s.opened = Some(s.recent[i].clone());
                }
            },
            state,
        );
        harness.run_steps(3);
        let typed = |h: &mut egui_kittest::Harness<'_, State>, text: &str| {
            h.input_mut().events.push(Event::Text(text.to_owned()));
            h.run_steps(2);
        };
        typed(&mut harness, "br");
        assert_eq!(harness.state().filter.query, "br");
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert_eq!(harness.state().filter.query, "", "Esc didn't clear it");
        // Still has the keyboard: typing goes on filtering.
        typed(&mut harness, "b");
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().opened, Some(PathBuf::from("/x/bravo")));
        // Typed and Enter in one frame: Enter goes by what was just typed.
        harness.state_mut().opened = None;
        harness.state_mut().filter.clear();
        harness.run_steps(1);
        harness
            .input_mut()
            .events
            .push(Event::Text("al".to_owned()));
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().opened, Some(PathBuf::from("/x/alpha")));
    }
}
