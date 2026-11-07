//! The tab strip: document tabs that drag to reorder, `+`, and the buttons at its end.

mod tab;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, Galley, Rect, Sense, Ui, UiBuilder, pos2, vec2};

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
/// Space between the tabs and the strip's buttons with the system's title bar.
const GUTTER: f32 = 12.0;
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
}

impl App {
    /// PhotoCraft-style document tabs on a dark strip, `+` after them, the theme menu at the end.
    /// A tab can be dragged along the strip, the others sliding out of its way.
    pub(super) fn tab_strip(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let mut actions = StripActions::default();
        let title_bar = super::title_bar::wanted(&self.settings);
        let strip = ui.available_rect_before_wrap();
        let strip = Rect::from_min_size(strip.min, vec2(strip.width(), TAB_H));

        // From the right: the caption buttons, the strip's own buttons against them, room to
        // move the window, and the tabs in what is left, those that don't fit in a list.
        let mut right = strip.right();
        if title_bar {
            let width = super::title_bar::captions_width(ui.ctx());
            super::title_bar::captions(ui, &t, strip.with_min_x(right - width));
            right -= width;
        }
        let gutter = if title_bar {
            super::title_bar::DRAG_GUTTER
        } else {
            GUTTER
        };
        let titles = self.titles(ui, &t);
        // When the buttons would leave the tabs too little room, they go into the profile menu.
        let top = self.title_bar_buttons();
        let buttons_w = top.len() as f32 * BAR_BUTTON_W + PROFILE_MAX_W;
        let folded = right - strip.left() - gutter - buttons_w < tabs_needed(&titles, self.active);
        let right_to_left = egui::Layout::right_to_left(egui::Align::Center);
        let buttons = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(strip.with_max_x(right))
                    .layout(right_to_left),
                |ui| self.bar_buttons(ui, &t, folded),
            )
            .response
            .rect;
        let tabs_area = strip.with_max_x((buttons.left() - gutter).max(strip.left()));
        let left_to_right = egui::Layout::left_to_right(egui::Align::Center);
        let tabs = ui.scope_builder(
            UiBuilder::new().max_rect(tabs_area).layout(left_to_right),
            |ui| {
                ui.set_clip_rect(tabs_area.intersect(ui.clip_rect()));
                ui.spacing_mut().item_spacing.x = GAP;
                let widths: Vec<f32> = titles.iter().map(|title| title.width).collect();
                let room = tabs_area.width() - PLUS_W;
                let mut shown = shown_tabs(&widths, self.active, room);
                if shown.len() < widths.len() {
                    shown = shown_tabs(&widths, self.active, room - LIST_W);
                }
                self.tabs_row(ui, &t, &mut actions, &titles, &shown);
                ui.add_space(4.0);
                if icons::small_button(ui, "plus", "New tab (Ctrl+T)").clicked() {
                    actions.add = true;
                }
                if shown.len() < titles.len() {
                    self.tabs_list(ui, &titles, &shown, &mut actions);
                }
            },
        );
        if title_bar {
            // What is left between the tabs and the buttons moves the window.
            let from = tabs.response.rect.right().min(buttons.left());
            let rest = Rect::from_min_max(pos2(from, strip.top()), buttons.left_bottom());
            super::title_bar::drag_area(ui, rest, "between");
        }
        self.apply_strip_actions(actions, ui.ctx());
    }

    /// The tabs that don't fit on the strip, in a list to pick one from.
    fn tabs_list(
        &self,
        ui: &mut Ui,
        titles: &[Title],
        shown: &[usize],
        actions: &mut StripActions,
    ) {
        ui.add_space(4.0);
        let hidden = titles.len() - shown.len();
        let resp = icons::small_button(ui, "chevron-down", &format!("{hidden} more tabs"));
        egui::Popup::menu(&resp)
            .style(theme::menu_style)
            .show(|ui| {
                for (i, title) in titles.iter().enumerate() {
                    if !shown.contains(&i) && ui.button(title.galley.text()).clicked() {
                        actions.select = Some(i);
                    }
                }
            });
    }

    /// The tabs `shown` in their places, the dragged one following the pointer over the others.
    fn tabs_row(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        actions: &mut StripActions,
        titles: &[Title],
        shown: &[usize],
    ) {
        let ctx = ui.ctx().clone();
        // Places on the strip, left to right; `shown` says which tab is in each.
        let n = shown.len();
        let width = |place: usize| {
            shown
                .get(place)
                .and_then(|&i| titles.get(i))
                .map_or(0.0, |g| g.width)
        };
        let total: f32 = (0..n).map(|p| width(p) + GAP).sum::<f32>() - GAP;
        let (strip, _) = ui.allocate_exact_size(vec2(total.max(0.0), TAB_H), Sense::hover());

        // The dragged tab follows the pointer; the slot it would drop into is where its
        // middle is among the others.
        let dragged = self.tab_drag.and_then(|d| {
            let from = shown.iter().position(|&i| i == d.from)?;
            let x = ctx.pointer_interact_pos()?.x - d.grab;
            let max = (strip.right() - width(from)).max(strip.left());
            Some((from, x.clamp(strip.left(), max)))
        });
        let (place, slot) = places(strip.left(), n, width, dragged);
        if let (Some((from, _)), Some(slot)) = (dragged, slot)
            && !ctx.input(|i| i.pointer.primary_down())
            && let (Some(&from), Some(&to)) = (shown.get(from), shown.get(slot))
        {
            actions.moved = Some((from, to));
        }

        // The dragged tab is drawn last, over the others.
        let dragged_place = dragged.map(|(from, _)| from);
        let paint_order = (0..n)
            .filter(|&p| Some(p) != dragged_place)
            .chain(dragged_place);
        let ids = self.tab_ids();
        let mut env = TabEnv {
            active: self.active,
            dragging: dragged_place.and_then(|p| shown.get(p).copied()),
            can_reopen: !self.closed.is_empty(),
            tab_looks: &self.tab_looks,
            switcher_query: &mut self.switcher_query,
            switcher_focus: &mut self.switcher_focus,
            t,
            actions,
        };
        for p in paint_order {
            let Some(&i) = shown.get(p) else {
                continue;
            };
            let (Some(tab), Some(title), Some(&id)) = (self.tabs.get(i), titles.get(i), ids.get(i))
            else {
                continue;
            };
            let x = match dragged {
                Some((from, x)) if from == p => ctx.animate_value_with_time(id.with("x"), x, 0.0),
                _ => ctx.animate_value_with_time(
                    id.with("x"),
                    place.get(p).copied().unwrap_or(strip.left()),
                    TAB_SLIDE,
                ),
            };
            let rect = Rect::from_min_size(pos2(x, strip.top()), vec2(title.width, TAB_H));
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

    /// Each tab's title and width.
    fn titles(&self, ui: &Ui, t: &Tokens) -> Vec<Title> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let active = i == self.active;
                let (title, chip, background): (String, Option<Chip>, usize) = match tab {
                    Tab::Home => ("New tab".to_owned(), None, 0),
                    Tab::Repo(r) => {
                        let label = r.label(self.settings.worktrees_in_same_tab);
                        let chip = worktree_tab::chip_for(ui, &label, MAX_CHIP_W);
                        (label.repo, chip, r.background_work().len())
                    }
                    Tab::Settings => ("Settings".to_owned(), None, 0),
                };
                let font = if active {
                    theme::medium(12.5)
                } else {
                    egui::FontId::proportional(12.5)
                };
                let color = if active { t.text } else { t.text_dim };
                let galley = ui.painter().layout_no_wrap(title, font, color);
                let extras = chip.as_ref().map_or(0.0, |c| c.width + 6.0)
                    + if background > 0 { BADGE_W } else { 0.0 };
                let width = (galley.size().x + 66.0 + extras).clamp(MIN_TAB_W, MAX_TAB_W);
                Title {
                    galley,
                    color,
                    width,
                    chip,
                    background,
                }
            })
            .collect()
    }

    /// Ids that stay with the tabs as they move.
    fn tab_ids(&self) -> Vec<egui::Id> {
        let mut homes = 0;
        self.tabs
            .iter()
            .map(|tab| match tab {
                Tab::Repo(r) => egui::Id::new(("tab", &r.path)),
                Tab::Home => {
                    homes += 1;
                    egui::Id::new(("tab-home", homes))
                }
                Tab::Settings => egui::Id::new("tab-settings"),
            })
            .collect()
    }

    /// The title bar's buttons, then the profile in use; `folded`, the buttons are in the
    /// profile's menu instead.
    fn bar_buttons(&mut self, ui: &mut Ui, t: &Tokens, folded: bool) {
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        let fixed = self.buttons_elsewhere();
        if !folded {
            app_buttons::area(ui, Place::TitleBar, Look::Bar, Flow::RightToLeft, &fixed);
        }
        let in_menu = if folded {
            self.title_bar_buttons()
        } else {
            Vec::new()
        };
        self.profile_button(ui, t, &in_menu);
    }

    /// Every button the title bar shows: its own, and the others' on a tab with no side panels.
    fn title_bar_buttons(&self) -> Vec<AppButton> {
        let layout = self.settings.button_layout.normalized();
        let mut buttons = layout.title_bar.clone();
        buttons.extend(self.buttons_elsewhere());
        buttons
    }

    /// The buttons placed in the side panels' footers when the active tab has none (the start
    /// page, the settings, a repository still opening): the title bar shows them too.
    fn buttons_elsewhere(&self) -> Vec<AppButton> {
        let panels = matches!(self.tabs.get(self.active), Some(Tab::Repo(r)) if !r.opening());
        if panels {
            return Vec::new();
        }
        let layout = self.settings.button_layout.normalized();
        [Place::BottomLeft, Place::BottomRight]
            .into_iter()
            .flat_map(|p| layout.at(p).to_vec())
            .collect()
    }

    fn apply_strip_actions(&mut self, actions: StripActions, ctx: &egui::Context) {
        if let Some(i) = actions.select {
            self.active = i;
        }
        // A tab closed as another is dropped: the indices are from before either, so the drop
        // is left out rather than moving the wrong tab or closing one that moved.
        if let Some((from, to)) = actions.moved
            && actions.close.is_none()
        {
            self.move_tab(from, to);
        }
        if let Some((path, look)) = actions.restyle {
            if look == TabLook::default() {
                self.tab_looks.remove(&path);
            } else {
                self.tab_looks.insert(path, look);
            }
        }
        if let Some(i) = actions.close {
            self.close_tab(i, ctx);
        }
        if actions.add {
            self.new_tab();
        }
        if actions.reopen {
            self.reopen_closed_tab(ctx);
        }
        // Opened and closed after the strip is drawn.
        if actions.switch_to.is_some() {
            self.switch_to = actions.switch_to;
        }
        if actions.close_worktree {
            self.close_worktree = true;
        }
    }
}

/// The room the tabs need at the least, below which the title bar's buttons give theirs up: the
/// active tab, one more at its narrowest, `+` and the list of those that don't fit.
fn tabs_needed(titles: &[Title], active: usize) -> f32 {
    let active_w = titles.get(active).map_or(0.0, |t| t.width);
    let another = if titles.len() > 1 {
        GAP + MIN_TAB_W + LIST_W
    } else {
        0.0
    };
    active_w + another + PLUS_W
}

/// The tabs that fit in `room` given their `widths`, in order: as many as fit from the first,
/// and the `active` one always, in place of those before it it needs the room of.
fn shown_tabs(widths: &[f32], active: usize, room: f32) -> Vec<usize> {
    let span = |tabs: &[usize]| -> f32 {
        let gaps = tabs.len().saturating_sub(1) as f32 * GAP;
        tabs.iter().filter_map(|&i| widths.get(i)).sum::<f32>() + gaps
    };
    let mut shown: Vec<usize> = Vec::new();
    for i in 0..widths.len() {
        shown.push(i);
        if span(&shown) > room {
            shown.pop();
            break;
        }
    }
    if active < widths.len() && !shown.contains(&active) {
        while !shown.is_empty() && span(&[shown.as_slice(), &[active]].concat()) > room {
            shown.pop();
        }
        shown.push(active);
    }
    shown
}

/// Each tab's left edge from `left` on, the dragged one (index, left edge) moved to the slot
/// its middle is over, and that slot.
fn places(
    left: f32,
    n: usize,
    width: impl Fn(usize) -> f32,
    dragged: Option<(usize, f32)>,
) -> (Vec<f32>, Option<usize>) {
    let mut order: Vec<usize> = (0..n).collect();
    let mut slot = None;
    if let Some((from, x)) = dragged {
        let middle = x + width(from) / 2.0;
        let mut edge = left;
        let mut s = 0;
        for j in (0..n).filter(|&j| j != from) {
            if edge + width(j) / 2.0 < middle {
                s += 1;
            }
            edge += width(j) + GAP;
        }
        order.retain(|&j| j != from);
        order.insert(s, from);
        slot = Some(s);
    }
    let mut place = vec![left; n];
    let mut x = left;
    for &j in &order {
        if let Some(p) = place.get_mut(j) {
            *p = x;
        }
        x += width(j) + GAP;
    }
    (place, slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Room for exactly `n` tabs of 100.
    fn room(n: usize) -> f32 {
        n as f32 * 100.0 + (n - 1) as f32 * GAP
    }

    #[test]
    fn all_tabs_show_when_they_fit() {
        assert_eq!(shown_tabs(&[100.0; 4], 0, room(4)), [0, 1, 2, 3]);
    }

    #[test]
    fn the_tabs_stop_where_the_room_ends() {
        assert_eq!(shown_tabs(&[100.0; 6], 1, room(3) + 50.0), [0, 1, 2]);
    }

    #[test]
    fn the_active_tab_shows_in_place_of_the_last_that_fit() {
        assert_eq!(shown_tabs(&[100.0; 6], 5, room(3)), [0, 1, 5]);
    }

    #[test]
    fn a_wide_active_tab_takes_the_room_of_several() {
        let widths = [100.0, 100.0, 100.0, 100.0, 250.0];
        assert_eq!(shown_tabs(&widths, 4, room(4)), [0, 4]);
    }

    #[test]
    fn the_active_tab_shows_even_alone_in_too_little_room() {
        assert_eq!(shown_tabs(&[100.0; 3], 2, 50.0), [2]);
    }

    #[test]
    fn no_tabs_none_shown() {
        assert!(shown_tabs(&[], 0, 500.0).is_empty());
    }
}
