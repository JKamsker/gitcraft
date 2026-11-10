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
        let count = if split {
            data.split.len()
        } else {
            data.lines.len()
        };
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

    /// One side-by-side row: the old line on the left half, the new one on the right, each
    /// `half` wide within the visible `view` and scrolled sideways by up to `overflow`.
    fn split_row(
        &mut self,
        ui: &mut Ui,
        row: usize,
        view: Rect,
        dimensions: (f32, f32, f32),
        acts: &mut Vec<Action>,
    ) {
        let (half, overflow, height) = dimensions;
        let Some(&(l, r)) = self.data.split.get(row) else {
            return;
        };
        let (rect, _) = ui.allocate_exact_size(vec2(half * 2.0 + overflow, height), Sense::hover());
        let scroll_x = (view.left() - rect.left()).clamp(0.0, overflow);
        let left =
            Rect::from_min_size(pos2(rect.left() + scroll_x, rect.top()), vec2(half, height));
        let right = Rect::from_min_size(pos2(left.right(), rect.top()), vec2(half, height));
        // Hunk headers and notes span both halves.
        if l == r
            && let Some(i) = l
            && matches!(
                self.data.lines.get(i).map(|x| x.kind),
                Some(Kind::Hunk | Kind::Meta | Kind::Fold)
            )
        {
            self.line(ui, left.union(right), i, Side::Both, 0.0, acts);
            return;
        }
        match l {
            Some(i) => self.line(ui, left, i, Side::Old, scroll_x, acts),
            None => filler(ui, left, &self.t),
        }
        match r {
            Some(i) => self.line(ui, right, i, Side::New, scroll_x, acts),
            None => filler(ui, right, &self.t),
        }
        ui.painter().line_segment(
            [right.left_top(), right.left_bottom()],
            Stroke::new(1.0, self.t.separator),
        );
    }

    /// Line `i` in `rect`, where it shows on `side`, with what can be done on it.
    fn line(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        i: usize,
        side: Side,
        scroll_x: f32,
        acts: &mut Vec<Action>,
    ) {
        let data = self.data;
        let kind = data.lines.get(i).map(|l| l.kind);
        let pickable = self.partial && matches!(kind, Some(Kind::Add | Kind::Del));
        let spot = self.comment_spot(i, side);
        let active = spot.is_some_and(|spot| self.is_target(spot));
        // A line a pull-request comment is written on shows picked; one a note is written on
        // doesn't, in a diff where picking a line is staging it.
        let local = self.review.is_some_and(|r| r.local);
        let picked = self.view.chosen.contains(&i) || (active && !local);
        // Hunk headers aren't copied, so they don't look selected either.
        let sel_chars = self
            .text_sel
            .and_then(|s| s.chars_of(i, side))
            .filter(|_| kind.is_some_and(|k| side.shows(k)));
        let found = self
            .find
            .and_then(|f| f.marks(i))
            .filter(|_| kind.is_some_and(|k| side.shows(k)));
        let placed = paint_line(
            ui,
            rect,
            data,
            i,
            side,
            scroll_x,
            &self.font,
            &self.t,
            picked,
            sel_chars,
            found,
            self.geometry.as_ref().map(|g| g.text_width),
        );
        if let Some(who) = data.authors.get(i).copied().flatten() {
            crate::repo::authorship_view::stripe(ui.painter(), &self.t, rect, who, 4.0);
        }
        if self.view.keyboard_hunk == Some(i) && self.data.lines[i].kind == Kind::Hunk {
            ui.painter().with_clip_rect(ui.clip_rect()).rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(2.0, self.t.accent),
                egui::StrokeKind::Inside,
            );
        }
        // Where the drag started and where the pointer is, in this line's text.
        if let Some((origin, galley)) = placed.filter(|_| kind.is_some_and(|k| side.shows(k))) {
            let text_rect = rect.with_min_x(rect.left() + gutter_width(side) + MARK_W);
            self.hit_test(
                text_rect.intersect(ui.clip_rect()),
                (i, side),
                origin,
                &galley,
            );
            if let Some(s) = self.text_sel.filter(|s| s.side == side && s.is_empty())
                && s.cursor.0 == i
            {
                let caret = galley.pos_from_cursor(egui::text::CCursor::new(s.cursor.1));
                let caret = caret.translate(origin.to_vec2());
                ui.painter()
                    .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                    .line_segment(
                        [caret.left_top(), caret.left_bottom()],
                        Stroke::new(1.0, self.t.text),
                    );
            }
        }
        if pickable {
            let marker = Rect::from_min_size(
                pos2(rect.left() + gutter_width(side), rect.top()),
                vec2(MARK_W, rect.height()),
            );
            let resp = ui
                .interact(marker, ui.id().with(("diff_line", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(crate::i18n::text("diff-pick-line"));
            crate::a11y::a11y_selected(&resp, || line_label(&data.lines[i]), picked);
            // A right-click on a changed line not picked picks it, for its menu to be about it.
            let unpicked = !self.view.chosen.contains(&i);
            if resp.clicked() || resp.secondary_clicked() && unpicked {
                self.clicked = Some(i);
            }
            self.picks.push(resp);
        }
        {
            let resp = ui.interact(
                rect,
                ui.id().with(("diff_read_line", i, column_of(side))),
                Sense::hover(),
            );
            crate::a11y::a11y(&resp, egui::WidgetType::Label, || {
                line_label(&data.lines[i])
            });
        }
        // Numbers take comments independently of both marker and code interaction.
        if let Some(spot) = spot {
            self.comment_gutter(ui, rect, i, side, spot, acts);
        }
        let mut right = rect.right().min(ui.clip_rect().right()) - 6.0;
        if kind == Some(Kind::Hunk) && !self.hunk_picks.is_empty() {
            right = self.hunk_buttons(ui, rect, i, acts) - 8.0;
        }
        if kind == Some(Kind::Hunk)
            && let DiffKey::Commit { hash, path } = &self.view.key
        {
            let next = data.lines[i + 1..]
                .iter()
                .take_while(|l| l.kind != Kind::Hunk)
                .collect::<Vec<_>>();
            let new = next
                .iter()
                .filter(|l| l.kind == Kind::Add)
                .filter_map(|l| l.new)
                .collect::<Vec<_>>();
            let old = next
                .iter()
                .filter(|l| l.kind == Kind::Del)
                .filter_map(|l| l.old)
                .collect::<Vec<_>>();
            let alternate = if !new.is_empty() {
                old.first()
                    .zip(old.last())
                    .map(|(&first, &last)| (first, last))
            } else {
                None
            };
            let (numbers, is_old) = if new.is_empty() {
                (old, true)
            } else {
                (new, false)
            };
            if let (Some(first), Some(last)) = (numbers.first(), numbers.last()) {
                let (button, left) = text_button(
                    ui,
                    right,
                    rect.center().y,
                    crate::i18n::text("repo-core-2306"),
                    self.t.accent,
                    ui.id().with(("prompt-hunk", i)),
                );
                if button
                    .on_hover_text(crate::i18n::text("repo-core-2308"))
                    .clicked()
                {
                    acts.push(Action::ShowPromptHunk(crate::repo::prompt_session::Hunk {
                        commit: hash.clone(),
                        path: path.clone(),
                        range: (*first, *last),
                        old: is_old,
                        alternate,
                    }));
                }
                right = left - 8.0;
            }
        }
        if kind == Some(Kind::Hunk)
            && let Some(tip) = data.weakened.get(&i)
        {
            super::weakened::badge(ui, right, rect.center().y, tip, i);
            right -= 24.0;
        }
        if matches!(kind, Some(Kind::Hunk | Kind::Fold)) {
            // Right of where the header's own text ends.
            let gutter = if side == Side::Unified {
                NUM_W * 2.0
            } else {
                0.0
            };
            let text = data.lines.get(i).map_or(String::new(), |l| l.text.clone());
            let text_w = ui
                .painter()
                .layout_no_wrap(text, self.font.clone(), self.t.text)
                .size()
                .x;
            let left = (rect.left() + gutter + 24.0 + text_w + 12.0).max(ui.clip_rect().left());
            super::fold_view::controls(ui, data, i, (left, right, rect.center().y), acts);
        }
        if kind.is_some_and(|k| side.shows(k)) {
            self.moved_button(ui, rect, i, acts);
        }
    }

    /// On the first line of each place of a moved block: where its other place is, and a click
    /// to see the two together.
    fn moved_button(&self, ui: &mut Ui, rect: Rect, i: usize, acts: &mut Vec<Action>) {
        let moves = &self.data.moves;
        let Some((b, _)) = moves.at(i) else { return };
        let Some(block) = moves.blocks.get(b) else {
            return;
        };
        let lines = &self.data.lines;
        let number = |at: usize, old: bool| {
            lines
                .get(at)
                .and_then(|l| if old { l.old } else { l.new })
                .unwrap_or(0)
        };
        let label = if i == block.del.start {
            crate::i18n::message(
                "repo-core-2309",
                &[(
                    "false",
                    format!("{}", number(block.add.start, false)).into(),
                )],
            )
        } else if i == block.add.start {
            crate::i18n::message(
                "repo-core-2310",
                &[("true", format!("{}", number(block.del.start, true)).into())],
            )
        } else {
            return;
        };
        let label = if block.exact {
            label
        } else {
            crate::i18n::message(
                "repo-core-2311",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&label).into(),
                )],
            )
        };
        let right = rect.right().min(ui.clip_rect().right()) - 6.0;
        let id = ui.id().with(("moved_btn", i));
        let (resp, _) = text_button(ui, right, rect.center().y, &label, self.t.accent, id);
        // The block whose places the panel below shows.
        if self.view.moved.as_ref().is_some_and(|o| o.block == b) {
            ui.painter().rect_stroke(
                resp.rect,
                self.t.radius_sm,
                Stroke::new(1.5, self.t.accent),
                egui::StrokeKind::Inside,
            );
        }
        if resp
            .on_hover_text(crate::i18n::text("repo-core-2313"))
            .clicked()
        {
            acts.push(Action::DiffMoved(Some(b)));
        }
    }

    /// The side and number line `i` takes a comment on where it shows on `side`, when a pull
    /// request's file is shown.
    fn comment_spot(&self, i: usize, side: Side) -> Option<(DiffSide, u64)> {
        let review = self.review?;
        let column = column_of(side);
        self.data
            .lines
            .get(i)
            .filter(|_| side != Side::Both)
            .and_then(|l| match column {
                None => unified_spot(l.kind, l.old, l.new, &review.marks),
                _ => target_of(l.kind, l.old, l.new, column),
            })
    }

    /// Whether the comment being written is on `spot`.
    fn is_target(&self, (side, line): (DiffSide, u64)) -> bool {
        self.review
            .and_then(|r| r.target.as_ref())
            .is_some_and(|t| t.side == side && t.line == line)
    }

    /// Notes where the press and the pointer are in the text of line `i` on `side`, laid out
    /// in `galley` from `origin`.
    fn hit_test(&mut self, rect: Rect, (i, side): (usize, Side), origin: Pos2, galley: &Galley) {
        let hit = |p: Pos2| (i, side, galley.cursor_from_pos(p - origin).index.0);
        if let Some(p) = self.press.filter(|p| rect.contains(*p)) {
            self.press_hit = Some(hit(p));
        }
        if let Some(p) = self.pointer.filter(|p| rect.y_range().contains(p.y)) {
            let over = rect.x_range().contains(p.x);
            self.pointer_hits.push((hit(p), over));
            self.text_hover |= over;
        }
    }

    /// The line's gutter in a pull request's file: its comments, and a click to write one.
    fn comment_gutter(
        &self,
        ui: &mut Ui,
        rect: Rect,
        i: usize,
        side: Side,
        (spot_side, number): (DiffSide, u64),
        acts: &mut Vec<Action>,
    ) {
        let Some(review) = self.review else { return };
        let gutter = if side == Side::Unified {
            NUM_W * 2.0
        } else {
            NUM_W
        };
        let area = Rect::from_min_size(rect.min, vec2(gutter, LINE_H));
        // Where a click picks the line (to stage it), a note takes Alt+click: the plain click
        // stays the line's.
        let alt_only = review.local && self.partial;
        let alt = ui.input(|i| i.modifiers.alt);
        let sense = if alt_only && !alt {
            Sense::hover()
        } else {
            Sense::click()
        };
        let tip = match (review.local, alt_only) {
            (false, _) => crate::i18n::text("repo-core-2314"),
            (true, false) => crate::i18n::text("repo-core-2315"),
            (true, true) => crate::i18n::text("repo-core-2316"),
        };
        let resp = ui.interact(
            area,
            ui.id().with(("diff_gutter", i, column_of(side))),
            sense,
        );
        let resp = if sense == Sense::click() {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            resp
        }
        .on_hover_text(tip);
        let count = review.marks.get(&(spot_side, number)).copied();
        let active = self.is_target((spot_side, number));
        crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
            crate::i18n::message(
                "repo-core-2318",
                &[
                    ("tip", tip.to_string().into()),
                    ("number", format!("{}", number).into()),
                    ("count", (count.unwrap_or(0)).into()),
                ],
            )
        });
        paint_gutter_mark(ui, area, count, resp.hovered(), active);
        if resp.clicked() {
            let target = LineTarget {
                path: review.path.clone(),
                line: number,
                side: spot_side,
            };
            acts.push(if review.local {
                Action::Agent(crate::repo::agents::AgentAct::OpenNote(target))
            } else {
                Action::Pr(PrAct::OpenLine(target))
            });
        }
    }

    /// The hunk actions in the header at line `i`, at the visible right end, however wide the
    /// line is.
    /// The hunk's buttons right-aligned in `rect`; returns where the leftmost one starts.
    fn hunk_buttons(&self, ui: &mut Ui, rect: Rect, i: usize, acts: &mut Vec<Action>) -> f32 {
        let mut right = rect.right().min(ui.clip_rect().right()) - 6.0;
        for &pick in self.hunk_picks.iter().rev() {
            let (resp, left) = hunk_button(ui, right, rect.center().y, pick, i);
            if resp.clicked() {
                acts.push(Action::PickLines {
                    pick,
                    lines: hunk_changes(&self.data.lines, i),
                });
            }
            right = left - 4.0;
        }
        right + 4.0
    }
}

/// The row line `i` shows on, unified or side by side (`split`).
pub(super) fn row_of(data: &DiffData, split: bool, i: usize) -> Option<usize> {
    if !split {
        return (i < data.lines.len()).then_some(i);
    }
    data.split
        .iter()
        .position(|&(l, r)| l == Some(i) || r == Some(i))
}

/// The half of the side-by-side view a side is, for comments (`None`: unified).
fn column_of(side: Side) -> Option<DiffSide> {
    match side {
        Side::Old => Some(DiffSide::Left),
        Side::New => Some(DiffSide::Right),
        Side::Unified | Side::Both => None,
    }
}

/// Width of the fixed line-number columns before the Git marker.
fn gutter_width(side: Side) -> f32 {
    match side {
        Side::Unified => NUM_W * 2.0,
        Side::Old | Side::New => NUM_W,
        Side::Both => 0.0,
    }
}

/// The painted line's identity, change kind, and content are available without hovering.
fn line_label(line: &super::DiffLine) -> String {
    let kind = match line.kind {
        Kind::Add => crate::i18n::text("repo-core-1702"),
        Kind::Del => crate::i18n::text("repo-core-2319"),
        Kind::Hunk => crate::i18n::text("repo-core-2320"),
        Kind::Context => crate::i18n::text("repo-core-2321"),
        Kind::Fold => "folded",
        Kind::Meta => "metadata",
    };
    crate::i18n::message(
        "repo-core-2322",
        &[
            ("kind", kind.to_string().into()),
            (
                "line",
                line.old
                    .map_or_else(
                        || crate::i18n::text("repo-core-2323").to_owned(),
                        |n| n.to_string(),
                    )
                    .to_string()
                    .into(),
            ),
            (
                "line2",
                line.new
                    .map_or_else(
                        || crate::i18n::text("repo-core-2323").to_owned(),
                        |n| n.to_string(),
                    )
                    .to_string()
                    .into(),
            ),
            ("text", line.text.to_string().into()),
        ],
    )
}

fn map_keyboard(ui: &Ui, offset: f32, page: f32, total: f32) -> Option<f32> {
    use egui::Key;
    for (key, value) in [
        (Key::Home, 0.0),
        (Key::End, total),
        (Key::ArrowUp, offset - LINE_H),
        (Key::ArrowDown, offset + LINE_H),
        (Key::PageUp, offset - page),
        (Key::PageDown, offset + page),
    ] {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
            return Some(value.clamp(0.0, (total - page).max(0.0)));
        }
    }
    None
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;

    #[test]
    fn actual_diff_map_publishes_scroll_range_and_applies_assistive_scroll() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let view = DiffView::new(DiffKey::Commit {
            hash: "recorded".into(),
            path: "file.rs".into(),
        });
        let data = DiffData {
            lines: (1..=100)
                .map(|line| super::super::DiffLine {
                    kind: Kind::Context,
                    old: Some(line),
                    new: Some(line),
                    text: "unchanged".into(),
                    raw: " unchanged".into(),
                })
                .collect(),
            spans: vec![None; 100],
            changed: vec![None; 100],
            ..Default::default()
        };
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (bool, f32)| {
                if !std::mem::replace(&mut state.0, true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let mut rows = Rows::new(ui, &view, &data, None, false, false);
                rows.show(ui, "file.rs", &mut Vec::new());
                state.1 = rows.offset;
                crate::a11y::end_frame(ui.ctx());
            },
            (false, 0.0),
        );
        h.run_steps(2);
        let node = h.get_by_role(egui::accesskit::Role::Slider);
        let data = node.accesskit_node().data();
        assert_eq!(data.numeric_value(), Some(0.0));
        assert_eq!(data.min_numeric_value(), Some(0.0));
        assert!(data.max_numeric_value().is_some_and(|max| max > 100.0));
        assert_eq!(
            data.orientation(),
            Some(egui::accesskit::Orientation::Vertical)
        );
        let (target_node, target_tree) = node.accesskit_node().locate();
        h.event(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::SetValue,
                target_node,
                target_tree,
                data: Some(egui::accesskit::ActionData::NumericValue(100.0)),
            },
        ));
        h.run();
        assert_eq!(h.state().1, 100.0);
        h.run_steps(2);
        assert_eq!(
            h.state().1,
            100.0,
            "actual ScrollArea state must retain the requested position"
        );
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(
            h.get_by_role(egui::accesskit::Role::Slider).is_focused(),
            "the text drag hit shield must not take keyboard focus"
        );
    }
    #[test]
    fn selection_registry_runs_actual_diff_rows_and_history_action() {
        use super::super::DiffLine;
        use crate::keymap::{self, Command, Scope, Stroke};
        use egui::{Event, Key, Modifiers};
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut settings = crate::settings::Settings::default();
        for (command, key) in [
            (Command::SelectionAll, Key::F8),
            (Command::SelectionCopy, Key::F9),
            (Command::SelectionHistory, Key::F10),
        ] {
            settings
                .key_bindings
                .insert(command.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
        }
        settings.put_for_test(&ctx);
        let mut view = DiffView::new(DiffKey::Commit {
            hash: "recorded".into(),
            path: "file.txt".into(),
        });
        let data = DiffData {
            lines: vec![
                DiffLine {
                    kind: Kind::Context,
                    old: Some(1),
                    new: Some(1),
                    text: "one".into(),
                    raw: " one".into(),
                },
                DiffLine {
                    kind: Kind::Add,
                    old: None,
                    new: Some(2),
                    text: "two".into(),
                    raw: "+two".into(),
                },
            ],
            spans: vec![None; 2],
            changed: vec![None; 2],
            ..Default::default()
        };
        for key in [None, Some(Key::F8), Some(Key::F9), Some(Key::F10)] {
            let mut acts = Vec::new();
            let out = ctx.run_ui(
                egui::RawInput {
                    events: key
                        .into_iter()
                        .flat_map(|key| {
                            [true, false].map(|pressed| Event::Key {
                                key,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            })
                        })
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    keymap::active(
                        &ctx,
                        vec![Scope::Window, Scope::Repository, Scope::Selection],
                    );
                    let mut rows = Rows::new(ui, &view, &data, None, false, false);
                    let selection = rows.show(ui, "file.txt", &mut acts);
                    drop(rows);
                    view.text_sel = selection;
                },
            );
            if key == Some(Key::F8) {
                assert_eq!(
                    view.text_sel
                        .map(|s| s.text(&data.lines, &data.uncut))
                        .as_deref(),
                    Some("one\ntwo")
                );
            }
            if key == Some(Key::F9) {
                assert!(
                    out.platform_output
                        .commands
                        .iter()
                        .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "one\ntwo"))
                );
            }
            if key == Some(Key::F10) {
                assert!(
                    matches!(acts.as_slice(), [Action::LineHistory { path, at, range: (1, 2) }] if path == "file.txt" && *at == crate::git::LinesAt::Rev("recorded".into()))
                );
            }
            out.drop_without_applying_deltas();
        }
    }

    #[test]
    fn change_map_keys_scroll_and_clamp_and_leave_modified_keys_alone() {
        use egui::{Key, Modifiers};
        for (key, modifiers, expected) in [
            (Key::ArrowDown, Modifiers::NONE, Some(100.0 + LINE_H)),
            (Key::PageUp, Modifiers::NONE, Some(0.0)),
            (Key::End, Modifiers::NONE, Some(800.0)),
            (Key::Home, Modifiers::NONE, Some(0.0)),
            (Key::ArrowDown, Modifiers::CTRL, None),
        ] {
            let ctx = egui::Context::default();
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(map_keyboard(ui, 100.0, 200.0, 1000.0), expected);
                },
            )
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn diff_names_include_change_kind_both_coordinates_and_content() {
        let line = super::super::DiffLine {
            kind: Kind::Add,
            old: None,
            new: Some(42),
            text: "let 日本語 = 1;".to_owned(),
            raw: "+let 日本語 = 1;".to_owned(),
        };
        assert_eq!(
            line_label(&line),
            "added, old line none, new line 42: let 日本語 = 1;"
        );
    }
}
