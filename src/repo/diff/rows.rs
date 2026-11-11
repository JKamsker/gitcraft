//! The diff's lines, unified or side by side, in a scroll area with the change map beside it:
//! changed lines and hunks to pick, gutters to comment from, text to select by dragging.

use egui::{FontId, Galley, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::jump::change_at;
use super::link::{LineLinks, note_right_click};
use super::map::change_map;
use super::paint::{filler, hunk_button, paint_line, text_button};
use super::select::{Hit, Side, TextSel, copy_selection, drag_selection, scroll_selection};
use super::{DiffData, Kind, LINE_H, MAP_W, MARK_W, NUM_W, Pick, hunk_changes};
use crate::git::Area;
use crate::pull_request::DiffSide;
use crate::repo::find::{Find, Hit as FoundHit, Seen};
use crate::repo::pr_review::{
    DiffReview, LineTarget, PrAct, paint_gutter_mark, target_of, unified_spot,
};
use crate::repo::{Action, DiffKey, DiffView};
use crate::theme::{self, Tokens};

/// What painting the visible lines needs, and what it found on them this frame.
pub(super) struct Rows<'a> {
    data: &'a DiffData,
    view: &'a DiffView,
    /// The pull request's comments, when its file is shown.
    review: Option<&'a DiffReview>,
    partial: bool,
    split: bool,
    wrap: bool,
    pub(super) geometry: Option<std::sync::Arc<super::wrap::Geometry>>,
    /// Which hunk actions the headers offer.
    hunk_picks: &'static [Pick],
    /// Only a selection made in the layout shown: one made side by side isn't on screen
    /// unified, and would be what Ctrl+C copies.
    text_sel: Option<TextSel>,
    /// Where the pointer was pressed, and where it is now.
    press: Option<Pos2>,
    pointer: Option<Pos2>,
    font: FontId,
    t: Tokens,
    /// A changed line clicked.
    pub(super) clicked: Option<usize>,
    /// A new code selection replaces any Git action selection.
    pub(super) text_started: bool,
    /// How far the lines are scrolled down.
    pub(super) offset: f32,
    pub(super) viewport_height: f32,
    pub(super) layout_changed: bool,
    /// The change scrolled to this frame (by its place among the changes).
    pub(super) jumped: Option<usize>,
    /// Text selection by dragging: where the press and the pointer are in the lines drawn.
    press_hit: Option<Hit>,
    /// Each with whether the pointer is over that line's half, not only at its height.
    pointer_hits: Vec<(Hit, bool)>,
    text_hover: bool,
    /// The find bar's search, and its match to scroll to this frame.
    find: Option<&'a Find>,
    reveal: Option<FoundHit>,
    /// Where the lines were scrolled this frame, for the find bar.
    pub(super) seen: Seen,
    /// Where the lines' menu links to, for a commit's diff.
    pub(super) links: Option<&'a LineLinks<'a>>,
    /// The changed lines that can be clicked: they take the right-click over them, so they
    /// open the lines' menu too.
    picks: Vec<egui::Response>,
}

impl<'a> Rows<'a> {
    /// Prepare row interaction state and Git actions for the current diff layout.
    pub(super) fn new(
        ui: &Ui,
        view: &'a DiffView,
        data: &'a DiffData,
        review: Option<&'a DiffReview>,
        partial: bool,
        split: bool,
    ) -> Self {
        let hunk_picks: &[Pick] = match &view.key {
            DiffKey::Work(f) if partial && f.area == Area::Staged => &[Pick::Unstage],
            DiffKey::Work(_) if partial => &[Pick::Stage, Pick::Discard],
            DiffKey::Commit { .. } if partial => &[Pick::Revert],
            _ => &[],
        };
        let (press, pointer) = ui.input(|i| (i.pointer.press_origin(), i.pointer.interact_pos()));
        Self {
            data,
            view,
            review,
            partial,
            split,
            wrap: crate::settings::Settings::get(ui.ctx()).diff_wrap,
            geometry: None,
            hunk_picks,
            text_sel: view.text_sel.filter(|s| s.made_in(split)),
            press,
            pointer,
            font: theme::mono(12.0),
            t: Tokens::get(ui.ctx()),
            clicked: None,
            text_started: false,
            offset: 0.0,
            viewport_height: 0.0,
            layout_changed: false,
            jumped: None,
            press_hit: None,
            pointer_hits: Vec::new(),
            text_hover: false,
            find: None,
            reveal: None,
            seen: Seen::default(),
            links: None,
            picks: Vec::new(),
        }
    }

    /// Marks the find bar's matches, and scrolls to `reveal`.
    pub(super) fn with_find(mut self, find: &'a Find, reveal: Option<FoundHit>) -> Self {
        self.find = Some(find);
        self.reveal = reveal;
        self
    }

    /// Shows the lines and the change map; returns the text selection after this frame's drag.
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        path: &str,
        acts: &mut Vec<Action>,
    ) -> Option<TextSel> {
        let data = self.data;
        let split = self.split;
        let char_w = ui
            .painter()
            .layout_no_wrap("M".to_owned(), self.font.clone(), self.t.text)
            .size()
            .x;
        let longest = data.longest as f32;

        ui.spacing_mut().item_spacing.y = 0.0;
        let count = super::presentation::DisplayRows::new(data, split).len();
        // Every change of the file in a strip beside the scrollbar, to see where to scroll to
        // (above all when the whole file is shown). Clicking a change's mark jumps to the
        // change, clicking elsewhere or dragging scrolls there.
        let (runs, changes) = data.runs(split);
        let area = ui.available_rect_before_wrap();
        let strip = Rect::from_min_max(pos2(area.right() - MAP_W, area.top()), area.max);
        let body = Rect::from_min_max(area.min, pos2(strip.left() - 1.0, area.bottom()));
        let geometry = self
            .wrap
            .then(|| super::wrap::geometry(ui, data, split, body.width() - 16.0, &self.font));
        self.geometry = geometry.clone();
        let total_h = geometry
            .as_ref()
            .map_or(count as f32 * LINE_H, |g| g.total());
        let offset = |row: usize| geometry.as_ref().map_or(row as f32 * LINE_H, |g| g.at(row));
        let map = ui
            .interact(
                strip,
                ui.id().with(("diff_map", path)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        // Code owns clicks and drags. Only the explicit marker gutter picks Git lines.
        let text_drag = ui.interact(
            body,
            ui.id().with(("diff_text", path)),
            Sense::CLICK | Sense::DRAG,
        );
        let mut scroll = if self.wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        }
        .id_salt(("diff_scroll", path, split))
        .auto_shrink(false);
        if let Some(i) = self.view.jump.filter(|&i| i < changes.len()) {
            self.jumped = Some(i);
        } else if (map.clicked() || map.dragged())
            && let Some(p) = map.interact_pointer_pos()
        {
            let frac = ((p.y - strip.top()) / strip.height()).clamp(0.0, 1.0);
            // A mark is at least 2 points tall; a little either side of it still hits it.
            let slack = 3.0 / strip.height() * count as f32;
            self.jumped = map
                .clicked()
                .then(|| {
                    change_at(
                        changes,
                        geometry
                            .as_ref()
                            .map_or(frac * count as f32, |g| g.row(frac * total_h) as f32),
                        slack,
                    )
                })
                .flatten();
            if self.jumped.is_none() {
                let at = frac * total_h;
                scroll = scroll.vertical_scroll_offset((at - body.height() / 2.0).max(0.0));
            }
        }
        if let Some(change) = self.jumped.and_then(|i| changes.get(i)) {
            scroll = scroll.vertical_scroll_offset(offset(change.start.saturating_sub(3)));
        } else if let Some(logical) = self.view.scroll_to {
            // Back where it was, read another way.
            scroll = scroll.vertical_scroll_offset(offset((logical / LINE_H).floor() as usize));
        }
        if let (Some(find), Some(hit)) = (self.find, self.reveal) {
            let text = data.lines.get(hit.line).map_or("", |l| l.text.as_str());
            if let Some(g) = &geometry {
                let galley = super::wrap::galley(ui, text, &self.font, g.text_width);
                let y = g.at(hit.row)
                    + galley
                        .pos_from_cursor(egui::text::CCursor::new(hit.start))
                        .top();
                scroll = scroll.vertical_scroll_offset((y - LINE_H * 3.0).max(0.0));
            } else {
                scroll = find.scroll_to(scroll, &hit, text, char_w);
            }
        } else if let Some(row) = self.view.jump_line.and_then(|i| row_of(data, split, i)) {
            scroll = scroll.vertical_scroll_offset(offset(row.saturating_sub(3)));
        }
        if map.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    map.id,
                    egui::EventFilter {
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            if let Some(offset) = map_keyboard(ui, self.view.scroll, body.height(), total_h) {
                scroll = scroll.vertical_scroll_offset(offset);
            }
        }
        let out = ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            if let Some(g) = &geometry {
                scroll.show_viewport(ui, |ui, viewport| {
                    scroll_selection(ui, &text_drag, self.wrap);
                    ui.set_min_height(total_h);
                    let start = g.row(viewport.top()).saturating_sub(1);
                    let end = (g.row(viewport.bottom()) + 2).min(count);
                    ui.add_space(g.at(start));
                    let view = ui.clip_rect();
                    let width = ui.available_width();
                    for row in start..end {
                        let h = g.height(row);
                        if split {
                            self.split_row(ui, row, view, ((width / 2.0).floor(), 0.0, h), acts);
                        } else {
                            let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
                            self.line(ui, rect, row, Side::Unified, 0.0, acts);
                        }
                    }
                })
            } else {
                scroll.show_rows(ui, LINE_H, count, |ui, range| {
                    scroll_selection(ui, &text_drag, self.wrap);
                    if split {
                        let view = ui.clip_rect();
                        let half = (view.width() / 2.0).floor();
                        let overflow = (NUM_W + 24.0 + longest * char_w + 16.0 - half).max(0.0);
                        for row in range {
                            self.split_row(ui, row, view, (half, overflow, LINE_H), acts);
                        }
                    } else {
                        let width = (NUM_W * 2.0 + 24.0 + longest * char_w + 24.0)
                            .max(ui.available_width());
                        for i in range {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(width, LINE_H), Sense::hover());
                            // Keep numbers and the Git marker in the visible gutter;
                            // only the code follows the horizontal scroll offset.
                            let scroll_x = (ui.clip_rect().left() - rect.left()).max(0.0);
                            let rect = rect.with_min_x(rect.left() + scroll_x);
                            self.line(ui, rect, i, Side::Unified, scroll_x, acts);
                        }
                    }
                })
            }
        });
        let mut shown = out.inner;
        if let Some(offset) = crate::a11y::value::slider(
            &map,
            crate::i18n::text("repo-core-2303"),
            shown.state.offset.y,
            0.0..=(total_h - shown.inner_rect.height()).max(0.0),
            LINE_H,
            true,
        ) {
            shown.state.offset.y = offset;
            shown.state.store(ui.ctx(), shown.id);
            ui.ctx().request_repaint();
        }
        self.offset = shown.state.offset.y;
        self.viewport_height = shown.inner_rect.height();
        self.layout_changed = super::presentation::KeyboardLayout {
            key: egui::Id::new((
                data.source,
                data.geometry_revision,
                split,
                self.wrap,
                body.width().to_bits(),
                self.font.clone(),
                ui.ctx().pixels_per_point().to_bits(),
            )),
            offsets: geometry.as_ref().map(|g| g.offsets.clone()),
            viewport_height: self.viewport_height,
        }
        .changed(ui.ctx(), ui.id().with(("keyboard-layout", path)));
        let view_w = shown.inner_rect.width();
        self.seen = Seen {
            x: shown.state.offset.x,
            y: super::wrap::logical_offset(
                geometry.as_ref().map(|g| g.offsets.as_slice()),
                shown.state.offset.y,
            ),
            height: geometry.as_ref().map_or(shown.inner_rect.height(), |g| {
                (g.row(shown.state.offset.y + shown.inner_rect.height())
                    .saturating_sub(g.row(shown.state.offset.y))
                    + 1) as f32
                    * LINE_H
            }),
            // The text's width beside the numbers: in each half side by side.
            text_w: if split {
                (view_w / 2.0).floor() - NUM_W - 24.0
            } else {
                view_w - NUM_W * 2.0 - 24.0
            },
            line_h: LINE_H,
        };
        change_map(
            ui,
            strip,
            runs,
            split,
            (shown.state.offset.y, shown.inner_rect.height(), total_h),
            geometry.as_ref().map(|g| g.offsets.as_slice()),
            &self.t,
        );

        // Select from where the drag started to the pointer, on the side it started on.
        let mut sel = drag_selection(
            ui,
            self.text_sel,
            &text_drag,
            self.press_hit,
            &self.pointer_hits,
            &data.lines,
        );
        self.text_started = ui.input(|i| i.pointer.primary_pressed())
            && text_drag.is_pointer_button_down_on()
            && self.press_hit.is_some()
            || text_drag.clicked() && self.pointer_hits.iter().any(|(_, over)| *over);
        if self.clicked.is_some() {
            sel = None;
        }
        if !crate::a11y::keyboard_taken(ui.ctx())
            && crate::keymap::take(ui.ctx(), crate::keymap::Command::SelectionAll)
        {
            let side = if !split {
                Side::Unified
            } else if sel.is_some_and(|s| s.side == Side::Old) {
                Side::Old
            } else {
                Side::New
            };
            sel = TextSel::all(&data.lines, side);
            self.text_started = sel.is_some();
            ui.ctx().request_repaint();
        }
        if self.text_hover || text_drag.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if text_drag.secondary_clicked() {
            let hit = self
                .pointer_hits
                .iter()
                .find(|(_, over)| *over)
                .map(|&((i, side, _), _)| (i, side));
            if sel.is_none_or(TextSel::is_empty)
                && let Some((i, side)) = hit
            {
                // Keep the code's menu about the clicked line without picking a Git target.
                sel = Some(TextSel {
                    side,
                    anchor: (i, 0),
                    cursor: (i, usize::MAX),
                });
                self.text_started = true;
            }
            if let Some(links) = self.links {
                note_right_click(ui, links.id(), hit);
            }
        }
        let links = self.links;
        let history = || {
            let old_path = super::old_name(&data.header);
            super::picked_lines(
                &self.view.key,
                old_path.as_deref(),
                &data.lines,
                sel.as_ref(),
                &self.view.chosen,
            )
            .map(|p| Action::LineHistory {
                path: p.path,
                at: p.at,
                range: p.range,
            })
        };
        let picks = std::mem::take(&mut self.picks);
        copy_selection(
            ui,
            &text_drag,
            &picks,
            sel,
            &data.lines,
            &data.uncut,
            |ui, acts| {
                if let Some(links) = links {
                    links.items(ui, (&data.lines, &data.header), sel, acts);
                }
            },
            &history,
            acts,
        );
        if sel != self.text_sel {
            ui.ctx().request_repaint();
        }
        sel
    }
}

/// The row line `i` shows on, unified or side by side (`split`).
pub(super) fn row_of(data: &DiffData, split: bool, i: usize) -> Option<usize> {
    super::presentation::DisplayRows::new(data, split).row_of(i)
}

#[cfg(test)]
#[path = "rows/tests.rs"]
mod accessibility_tests;

mod actions;

mod paint;

mod labels;
use labels::{column_of, gutter_width, line_label, map_keyboard};
