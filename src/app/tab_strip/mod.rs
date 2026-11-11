//! The tab strip: document tabs that drag to reorder, `+`, and the buttons at its end.

mod actions;
mod geometry;
use geometry::*;

mod tab;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, Galley, Rect, Ui, UiBuilder, pos2, vec2};

use super::{App, Tab};
use crate::app_buttons::{self, AppButton, Flow, Look, Place};
use crate::icons;
use crate::repo::worktree_tab::{self, Chip};
use crate::tab_look::TabLook;
use crate::theme::{self, Tokens};

const TAB_H: f32 = 40.0;
/// Space between tabs.
const GAP: f32 = 1.0;
/// Seconds tabs take to slide into their place.
const TAB_SLIDE: f32 = 0.15;
/// Wide enough for a repository, its branch's chip and the badge.
const MAX_TAB_W: f32 = 380.0;
/// The badge counting a tab's worktrees with work in the background.
const BADGE_W: f32 = 22.0;
/// The widest a worktree chip gets: what a tab at its widest leaves next to its icon, its close
/// button and the badge.
const MAX_CHIP_W: f32 = MAX_TAB_W - 32.0 - 28.0 - BADGE_W;

/// What `+` takes after the tabs: its space before it and the button.
const PLUS_W: f32 = 4.0 + 20.0 + GAP;
/// What the list of the tabs that don't fit takes after `+`.
const LIST_W: f32 = 4.0 + 20.0 + GAP;
/// The list of the tabs out of view (with `+` in it) at the end of the strip, with the space
/// around it.
const CONTROLS_W: f32 = 4.0 + 20.0 + 4.0;
/// Space between the tabs and the buttons at the end of the strip.
const END_GAP: f32 = 4.0;
/// The free strip below which the grip that moves the window shows.
const MIN_DRAG_ROOM: f32 = 40.0;
/// How wide the fade is where tabs are scrolled out of view.
const FADE_W: f32 = 28.0;
/// Seconds the tabs take to scroll where they are sent.
const SCROLL_TIME: f32 = 0.12;
/// A tab at its narrowest.
const MIN_TAB_W: f32 = 110.0;
/// A button of the title bar with the space after it.
const BAR_BUTTON_W: f32 = 28.0 + 6.0;
/// The profile button at its widest, with the space before the buttons.
const PROFILE_MAX_W: f32 = 180.0 + 4.0;

/// A tab being dragged along the strip.
#[derive(Clone, Copy)]
pub(super) struct TabDrag {
    /// Its index when the drag started.
    from: usize,
    /// Where the pointer holds it, from its left edge.
    grab: f32,
}

/// What was done on the strip this frame, applied once it is drawn.
#[derive(Default)]
struct StripActions {
    select: Option<usize>,
    close: Option<usize>,
    add: bool,
    restyle: Option<(PathBuf, TabLook)>,
    /// A dragged tab dropped: (from, to).
    moved: Option<(usize, usize)>,
    drag: Option<TabDrag>,
    /// The worktree picked in the switcher.
    switch_to: Option<PathBuf>,
    /// The active tab's worktree is to be closed.
    close_worktree: bool,
    /// The tab closed last is to be opened again.
    reopen: bool,
    quit: bool,
    /// The work of this repository tab is to be teleported.
    teleport: Option<PathBuf>,
    window: Option<(usize, Option<egui::ViewportId>, Option<egui::Pos2>)>,

    teleport_to: Option<(PathBuf, crate::teleport::Machine)>,
}

/// A tab's name, laid out, its colour, the tab's width, its worktree chip and how many of its
/// worktrees have work in the background.
struct Title {
    galley: Arc<Galley>,
    color: Color32,
    width: f32,
    chip: Option<Chip>,
    background: usize,
}

/// One tab where it is drawn this frame.
struct Placed<'a> {
    index: usize,
    tab: &'a Tab,
    title: &'a Title,
    rect: Rect,
    id: egui::Id,
}

/// What drawing a tab needs besides the tab: the app's state and what the strip collects.
struct TabEnv<'a> {
    active: usize,
    /// The tab being dragged, if any.
    dragging: Option<usize>,
    /// A closed tab can be opened again.
    can_reopen: bool,
    tab_looks: &'a BTreeMap<PathBuf, TabLook>,
    switcher_query: &'a mut String,
    switcher_focus: &'a mut bool,
    t: &'a Tokens,
    actions: &'a mut StripActions,
    windows: Vec<(egui::ViewportId, String)>,
    can_detach: bool,
}

impl App {
    /// PhotoCraft-style document tabs on a dark strip, `+` after them, the buttons at the end.
    /// A tab can be dragged along the strip, the others sliding out of its way. When they don't
    /// all fit, they scroll sideways under the end of the strip (wheel or touchpad), and `+` and
    /// a list of those out of view wait there.
    pub(super) fn tab_strip(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let mut actions = StripActions::default();
        let title_bar = super::title_bar::wanted(&self.settings);
        let strip = ui.available_rect_before_wrap();
        let strip = Rect::from_min_size(strip.min, vec2(strip.width(), TAB_H));
        self.window_strip(ui.ctx(), strip);
        let right_to_left = egui::Layout::right_to_left(egui::Align::Center);

        // From the right: the caption buttons, the strip's own buttons against them and the
        // profile, the grip that moves the window when the tabs leave no room for that, then
        // the tabs in what is left.
        let mut right = strip.right();
        if title_bar {
            let width = super::title_bar::captions_width(ui.ctx());
            super::title_bar::captions(ui, &t, strip.with_min_x(right - width));
            right -= width;
        }
        let titles = self.titles(ui, &t);
        // When the buttons would leave the tabs too little room, they go into the profile menu.
        let handle_w = if title_bar {
            super::title_bar::HANDLE_W
        } else {
            0.0
        };
        let top = self.title_bar_buttons();
        let buttons_w = top.len() as f32 * BAR_BUTTON_W + PROFILE_MAX_W + handle_w;
        let folded = right - strip.left() - END_GAP - buttons_w < tabs_needed(&titles, self.active);
        let buttons = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(strip.with_max_x(right))
                    .layout(right_to_left),
                |ui| self.bar_buttons(ui, &t, folded),
            )
            .response
            .rect;

        let widths: Vec<f32> = titles.iter().map(|title| title.width).collect();
        let total = row_width(&widths);
        // The grip only when the tabs leave too little of the strip free to move the window by.
        let free = buttons.left() - END_GAP - strip.left() - total - PLUS_W;
        let buttons_left = if title_bar && free < MIN_DRAG_ROOM {
            let rect = strip.with_max_x(buttons.left());
            let layout = UiBuilder::new().max_rect(rect).layout(right_to_left);
            ui.scope_builder(layout, |ui| {
                ui.add_space(2.0);
                super::title_bar::handle(ui, &t);
            })
            .response
            .rect
            .left()
        } else {
            buttons.left()
        };
        let end = (buttons_left - END_GAP).max(strip.left());
        let overflow = total + PLUS_W > end - strip.left();
        let controls = if overflow { CONTROLS_W } else { 0.0 };
        let area = Rect::from_min_max(
            strip.left_top(),
            pos2((end - controls).max(strip.left()), strip.bottom()),
        );
        let offset = self.scroll_tabs(ui, area, &widths);
        let row = Rect::from_min_size(pos2(area.left() - offset, strip.top()), vec2(total, TAB_H));

        let left_to_right = egui::Layout::left_to_right(egui::Align::Center);
        ui.scope_builder(
            UiBuilder::new().max_rect(area).layout(left_to_right),
            |ui| {
                ui.set_clip_rect(area.intersect(ui.clip_rect()));
                // The strip is as tall as a tab however the tabs are scrolled.
                ui.advance_cursor_after_rect(area);
                self.tabs_row(ui, &t, &mut actions, &titles, row, strip);
                // The ends fade where more tabs are scrolled out of view.
                let max = (total - area.width()).max(0.0);
                if offset > 0.5 {
                    let edge = area.with_max_x(area.left() + FADE_W);
                    fade(ui.painter(), edge, t.tab_strip, false);
                }
                if offset < max - 0.5 {
                    let edge = area.with_min_x(area.right() - FADE_W);
                    fade(ui.painter(), edge, t.tab_strip, true);
                }
            },
        );
        if overflow {
            // The list of the tabs out of view, `+` at its top, next to the grip.
            let rect =
                Rect::from_min_max(pos2(area.right(), strip.top()), pos2(end, strip.bottom()));
            let hidden = out_of_view(&widths, offset, area.width());
            ui.scope_builder(
                UiBuilder::new().max_rect(rect).layout(right_to_left),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    self.tabs_list(ui, &titles, &hidden, &mut actions);
                },
            );
        } else {
            let x = row.right() + 4.0;
            let rect = Rect::from_min_size(pos2(x, strip.top()), vec2(20.0, TAB_H));
            ui.scope_builder(
                UiBuilder::new().max_rect(rect).layout(left_to_right),
                |ui| {
                    if icons::small_button(
                        ui,
                        "plus",
                        &crate::keymap::tip(
                            ui.ctx(),
                            crate::i18n::text("interaction-tabs-new-tab"),
                            crate::keymap::Command::NewTab,
                        ),
                    )
                    .clicked()
                    {
                        actions.add = true;
                    }
                },
            );
            if title_bar {
                // What is left between `+` and the grip moves the window too.
                let rest =
                    Rect::from_min_max(pos2(x + 24.0, strip.top()), pos2(end, strip.bottom()));
                super::title_bar::drag_area(ui, rest, "between");
            }
        }
        self.apply_strip_actions(actions, ui.ctx());
    }

    /// How far the tabs are scrolled, given the room `area` they show in: moved by the wheel or
    /// touchpad over them, and so that the active tab is in view when it changes.
    fn scroll_tabs(&mut self, ui: &Ui, area: Rect, widths: &[f32]) -> f32 {
        let room = area.width();
        let max = (row_width(widths) - room).max(0.0);
        let mut offset = self.tab_scroll;
        if ui.rect_contains_pointer(area) {
            // A mouse wheel scrolls up and down; either way moves the tabs along.
            let delta = ui.input(|i| i.smooth_scroll_delta);
            offset -= delta.x + delta.y;
        }
        let active = self.tab_ids().get(self.active).copied();
        if self.tab_scrolled_to != active {
            self.tab_scrolled_to = active;
            if let Some((left, right)) = tab_span(widths, self.active) {
                offset = scroll_to_show(offset, room, left, right);
            }
        }
        self.tab_scroll = offset.clamp(0.0, max);
        ui.ctx()
            .animate_value_with_time(egui::Id::new("tab-scroll"), self.tab_scroll, SCROLL_TIME)
    }

    /// The tabs out of view, in a list to pick one from, after a new tab (the strip has no room
    /// left for `+`).
    fn tabs_list(
        &self,
        ui: &mut Ui,
        titles: &[Title],
        hidden: &[usize],
        actions: &mut StripActions,
    ) {
        let tip = crate::i18n::message(
            "interaction-tabs-overflow",
            &[("count", hidden.len().into())],
        );
        let resp = icons::small_button(ui, "chevron-down", &tip);
        egui::Popup::menu(&resp)
            .style(theme::menu_style)
            .show(|ui| {
                if crate::menu_item::MenuItem::new(crate::i18n::text("interaction-tabs-new-tab"))
                    .icon("plus")
                    .keys(crate::keymap::Command::NewTab)
                    .show(ui)
                    .clicked()
                {
                    actions.add = true;
                }
                ui.separator();
                for &i in hidden {
                    if let Some(title) = titles.get(i)
                        && ui.button(title.galley.text()).clicked()
                    {
                        actions.select = Some(i);
                    }
                }
            });
    }

    /// Every tab in its place along `row` (which may run past what is in view), the dragged one
    /// following the pointer over the others.
    fn tabs_row(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        actions: &mut StripActions,
        titles: &[Title],
        row: Rect,
        strip: Rect,
    ) {
        let ctx = ui.ctx().clone();
        let n = titles.len();
        let width = |i: usize| titles.get(i).map_or(0.0, |g| g.width);

        // The dragged tab follows the pointer; the slot it would drop into is where its
        // middle is among the others.
        let dragged = self.tab_drag.filter(|d| d.from < n).and_then(|d| {
            let x = ctx.pointer_interact_pos()?.x - d.grab;
            let max = (row.right() - width(d.from)).max(row.left());
            Some((d.from, x.clamp(row.left(), max)))
        });
        let source = self
            .tab_drag
            .and_then(|d| self.tabs.get(d.from))
            .and_then(|tab| match tab {
                Tab::Repo(repo) => Some(repo.path.clone()),
                _ => None,
            });
        let teleported = self.teleport_drop_target(&ctx, source);
        let accepted = teleported.is_some();
        actions.teleport_to = teleported;
        let (place, slot) = places(row.left(), n, width, dragged);
        if let (Some((from, _)), Some(slot)) = (dragged, slot)
            && !accepted
            && !ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !ctx.input(|i| i.pointer.primary_down())
        {
            if let Some((target, position)) = self.drop_window_target(from, strip, &ctx) {
                actions.window = Some((from, target, Some(position)));
            } else {
                actions.moved = Some((from, slot));
            }
        }

        // The dragged tab is drawn last, over the others.
        let dragging = dragged.map(|(from, _)| from);
        let paint_order = (0..n).filter(|&j| Some(j) != dragging).chain(dragging);
        let ids = self.tab_ids();
        let windows = self.window_targets();
        let can_detach = self.windows.enabled && !ctx.embed_viewports();
        let mut env = TabEnv {
            windows,
            can_detach,
            active: self.active,
            dragging,
            can_reopen: !self.closed.is_empty(),
            tab_looks: &self.tab_looks,
            switcher_query: &mut self.switcher_query,
            switcher_focus: &mut self.switcher_focus,
            t,
            actions,
        };
        for i in paint_order {
            let (Some(tab), Some(title), Some(&id)) = (self.tabs.get(i), titles.get(i), ids.get(i))
            else {
                continue;
            };
            // Places are kept from the row's start, so scrolling moves the tabs at once and
            // only a change of order slides.
            let x = match dragged {
                // Kept as its place, so that when let go it slides from there.
                Some((from, x)) if from == i => {
                    row.left() + ctx.animate_value_with_time(id.with("x"), x - row.left(), 0.0)
                }
                _ => {
                    let at = place.get(i).copied().unwrap_or(row.left()) - row.left();
                    row.left()
                        + ctx.animate_value_with_time(
                            id.with("x"),
                            at,
                            crate::a11y::motion::seconds(&ctx, TAB_SLIDE),
                        )
                }
            };
            let rect = Rect::from_min_size(pos2(x, row.top()), vec2(title.width, TAB_H));
            let placed = Placed {
                index: i,
                tab,
                title,
                rect,
                id,
            };
            tab::tab_ui(ui, placed, &mut env);
        }
        if let Some(drag) = env.actions.drag.take() {
            self.tab_drag = Some(drag);
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.tab_drag = None;
        }
    }
}

/// The room the tabs need at the least, below which the title bar's buttons give theirs up: the
/// active tab, one more at its narrowest, `+` and the list of those that don't fit.
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
