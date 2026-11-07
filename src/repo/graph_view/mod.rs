//! Central pane: the commit graph of every branch, with branch pills on the left, search and
//! older history loaded as you scroll.

mod badge;
mod drag;
mod handles;
mod header;
mod lanes;
mod menu;
pub(super) mod message_tooltip;
pub(super) mod minimap;
mod order;
mod pills;
mod readability;
#[cfg(test)]
mod readability_tests;
mod row;
mod search;
pub(super) mod suggest;
mod suggest_popup;

use egui::{Align2, FontId, Key, Response, Sense, Ui, vec2};

use super::actions::{BranchRef, compare_branches_action};
use super::columns::{Cols, MIN_COL_W, MIN_GRAPH_W, MIN_REFS_W};
use super::ghost::{Kind, Shift};
use super::{Action, Dialog, Refresh, RepoTab, WIP_HASH, ops};
use crate::git;
use crate::keymap::Command;
use crate::menu_item::MenuItem;
use crate::settings::Settings;
use crate::theme::Tokens;
pub(super) use badge::BadgeIndex;
use badge::{Badge, BadgeKind, badges};
/// For the settings' previews, which draw a graph of their own like this one.
pub(crate) use lanes::edge;
use row::Lanes;

// The reset modes, for the reflog view's entries too.
pub(super) use menu::{Rewrite, reset_items};

pub(crate) const ROW_H: f32 = 28.0;
/// Space between rows, which the graph's lines cross.
pub(crate) const ROW_GAP: f32 = 2.0;
/// From one row to the next.
pub(crate) const ROW_PITCH: f32 = ROW_H + ROW_GAP;
/// From one lane to the next.
pub(crate) const LANE_W: f32 = 16.0;
/// From one lane to the next in a compact graph column.
pub(crate) const COMPACT_LANE_W: f32 = 10.0;
/// Radius of the corner where a line from a dot turns into another lane.
const CORNER_R: f32 = 7.0;
const MAX_LANES_SHOWN: usize = 24;
/// How much darker the pills that are not checked out are (0 = same colour, 1 = black).
pub(crate) const DIMMED: f32 = 0.4;
/// A translucent black, so the column handles read as dark on every theme's header.
const HANDLE_COLOR: egui::Color32 = egui::Color32::from_black_alpha(130);
const HANDLE_WIDTH: f32 = 2.0;
/// How far a handle sits left of the column it resizes.
const HANDLE_GAP: f32 = 5.0;
const AVATAR_D: f32 = 20.0;
/// How opaque a commit a ghost run would make is drawn.
const GHOST_OPACITY: f32 = 0.6;
/// How opaque a commit it would leave on no ref is drawn.
const GOING_OPACITY: f32 = 0.4;

impl RepoTab {
    /// A comparison opened from a pull request focuses that branch, including when several
    /// branches share its tip and the ordinary ordering would put it behind `+N`.
    fn graph_badges(&self, commit: &git::Commit) -> Vec<Badge> {
        let mut badges = self.graph_badge_index.badges(&commit.refs);
        if let Some(compare) = &self.compare
            && compare.target.as_deref() == Some(commit.hash.as_str())
        {
            let focused = match badges.iter().position(|badge| {
                badge.kind == BadgeKind::Branch && badge.label == compare.target_label
            }) {
                Some(index) => badges.remove(index),
                None if self.prs.iter().any(|pr| pr.branch == compare.target_label) => Badge {
                    label: compare.target_label.clone(),
                    kind: BadgeKind::PullRequest,
                    local: false,
                    remotes: Vec::new(),
                    head: false,
                },
                None => return badges,
            };
            badges.insert(0, focused);
        }
        badges
    }

    /// The graph, or during a ghost run the graph as the operation would leave it (see
    /// [`super::ghost`]), with the strip that says what it would do above it.
    pub(super) fn graph_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let area = ui.available_rect_before_wrap();
        let ghost = self
            .ghost
            .swap_in(&mut self.commits, &mut self.graph, &mut self.index);
        // Rows are numbered differently in the ghost graph: the selected commit is found there
        // by its hash (the open menu's entries, "Compare with" among them, stay as they were).
        let selected = self.selected;
        if ghost {
            self.selected = self
                .ghost
                .real_hash(selected)
                .and_then(|h| self.index.get(&h).copied());
        }
        self.graph_rows_view(ui, acts);
        if ghost {
            self.ghost
                .swap_out(&mut self.commits, &mut self.graph, &mut self.index);
            self.selected = selected;
        }
        // Over the graph's foot rather than above it: the rows (and the menu open on one)
        // don't move as it comes and goes, or grows when the result is in.
        egui::Area::new(ui.id().with("ghost-strip"))
            .order(egui::Order::Middle)
            .interactable(false)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(area.left_bottom() + vec2(8.0, -8.0))
            .show(ui.ctx(), |ui| self.ghost_strip(ui, area.width() - 16.0));
    }

    fn graph_rows_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        self.graph_motion.expire();
        self.graph_motion.schedule_expiry(ui.ctx());
        self.celebration_ui(ui);
        self.graph_walk_header(ui);
        let map_area = ui.available_rect_before_wrap();
        let map_rect = egui::Rect::from_min_max(
            egui::pos2(map_area.right() - 78.0, map_area.top()),
            map_area.right_bottom(),
        );
        let show_map = map_area.width() > 480.0 && map_area.height() > 120.0;
        if show_map {
            ui.set_width((map_area.width() - 84.0).max(1.0));
        }
        let t = Tokens::get(ui.ctx());
        if self.search.open {
            self.search_bar(ui, acts);
        }
        if let Some(err) = &self.load_error {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.colored_label(t.danger, err);
            });
            return;
        }
        self.tip_bar(ui);
        let first_steps = self.first_steps();
        if self.commits.is_empty() {
            if let Some(steps) = first_steps {
                self.first_steps_view(ui, steps, acts);
                return;
            }
            ui.centered_and_justified(|ui| {
                if self.loading {
                    ui.add(crate::spinner::Spinner::new());
                } else {
                    ui.label(
                        egui::RichText::new("This repository has no commits yet.")
                            .color(t.text_dim),
                    );
                }
            });
            return;
        }

        // Keyboard navigation (only when no text field has focus, nor a window of the app's
        // is over the tab: the manual, the palette, the clone form).
        let ghost = self.ghost.drawing();
        // The rows are one Tab stop (see `a11y::list_stop`), whose ↑ / ↓ are these.
        let rows_id = ui.id().with("graph-rows");
        let keys_free =
            !ui.ctx().egui_wants_keyboard_input() || crate::a11y::list_has_focus(ui.ctx(), rows_id);
        if !ghost
            && keys_free
            && self.dialog.is_none()
            && self.package.is_none()
            && !self.stacks.modal_open()
            && !self.keys_blocked
        {
            let ringed = ui.data(|d| d.get_temp::<usize>(rows_id));
            self.arrow_keys(ui, ringed, acts);
        }

        let settings = Settings::get(ui.ctx());
        let lanes = lane_layout(&settings, self.graph.width);
        let mut prefs = preferred_cols(&settings, lanes);
        let mut cols = prefs.squeezed(ui.available_width(), settings.last_resized);

        // Column headers.
        let (header, header_resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::CLICK);

        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("graph_scroll")
            .auto_shrink(false);
        let offset_id = ui.id().with("graph_offset");
        let offset_now = ui.data(|d| d.get_temp::<f32>(offset_id)).unwrap_or(0.0);
        match self.ghost.take_shift() {
            // Rows added on top: the rows in view stay where they are, unless the top is shown.
            Some(Shift::In(rows)) if offset_now > 0.0 => {
                let by = rows as f32 * ROW_PITCH;
                self.ghost.shifted = by;
                scroll = scroll.vertical_scroll_offset(offset_now + by);
            }
            Some(Shift::Out(by)) => {
                scroll = scroll.vertical_scroll_offset((offset_now - by).max(0.0));
            }
            _ => {}
        }
        if let Some(row) = self.scroll_to.take_if(|_| !ghost) {
            let follow_head = self.graph_motion.take_head_scroll();
            let view_h = ui.available_height();
            let top = row as f32 * ROW_PITCH;
            // Only scroll if the row is outside the visible part.
            let offset = ui.data(|d| d.get_temp::<f32>(offset_id)).unwrap_or(0.0);
            if top < offset || top + ROW_PITCH > offset + view_h {
                let target = (top - view_h / 2.0).max(0.0);
                let offset = if follow_head {
                    self.graph_motion.start_scroll(offset, target, ui.ctx())
                } else {
                    self.graph_motion.cancel_scroll();
                    target
                };
                scroll = scroll.vertical_scroll_offset(offset);
            }
        }

        if ui.input(|i| i.smooth_scroll_delta.y != 0.0 || i.pointer.any_pressed()) {
            self.graph_motion.cancel_scroll();
        }
        if let Some(offset) = self.graph_motion.scroll_offset(ui.ctx()) {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        if let Some(offset) = self.history_minimap.offset.take() {
            self.graph_motion.cancel_scroll();
            scroll = scroll.vertical_scroll_offset(offset);
        }
        // `show_rows` sizes rows as ROW_PITCH + item spacing, so the spacing must be zero here.
        ui.spacing_mut().item_spacing.y = 0.0;
        let idle = self.busy_op.is_none();
        let total = self.commits.len() + usize::from(self.has_more);
        // The merges shown, for their badges: only those are read.
        let mut shown_merges = Vec::new();
        // A new repository's first steps go under its few rows.
        let first_steps = first_steps.filter(|_| !self.guide.steps_hidden);
        if first_steps.is_some() {
            scroll = scroll.max_height(total as f32 * ROW_PITCH);
        }
        self.test_lane_frame(&settings, ui.ctx());
        let mut hovered_message = None;
        let out = scroll.show_rows(ui, ROW_PITCH, total, |ui, range| {
            // show_rows includes overscan and partially clipped rows. Require the text's
            // centre to be inside the viewport with enough room for its glyphs: a label on
            // a mostly hidden top row would leave the visible lane looking unnamed.
            let top = ui.max_rect().top();
            let clip = ui.clip_rect();
            let mut visible = range.clone().filter(|i| {
                let y = top + (i - range.start) as f32 * ROW_PITCH;
                let center = y + ROW_PITCH / 2.0;
                center >= clip.top() + 8.0 && center <= clip.bottom() - 8.0
            });
            let first = visible.clone().next().unwrap_or(range.start);
            let end = visible.next_back().map_or(first, |i| i + 1);
            let readable = self.readable_viewport(ui, first..end, lanes.shown);
            if settings.fit_visible_refs() {
                prefs.refs = readable.refs_width;
            }
            cols = prefs.squeezed(ui.available_width(), settings.last_resized);
            // Near the end: fetch the next page of history.
            // Not from the ghost graph, whose rows don't grow as pages come in.
            if range.end + 40 >= self.commits.len() && self.has_more && !self.loading_more && !ghost
            {
                acts.push(Action::LoadMore);
            }
            self.drag_auto_scroll(ui);
            // Tab reaches the selected row, else the first in view.
            let stop = self
                .selected
                .filter(|s| range.contains(s))
                .unwrap_or(range.start);
            // Where ↑ / ↓ start from while nothing is selected: the row Tab rings.
            ui.data_mut(|d| d.insert_temp(rows_id, stop));
            let menu_row = ui.data(|d| d.get_temp::<String>(menu_row_id()));
            let mut menu_drawn = false;
            for i in range {
                if i >= self.commits.len() {
                    loading_row(ui, &t);
                    continue;
                }
                if !ghost {
                    if self.commits[i].parents.len() == 2 {
                        shown_merges.push(self.commits[i].hash.clone());
                    }
                    let (resp, hovered) =
                        self.commit_row(ui, i, cols, lanes, &readable, i == stop, acts);
                    if hovered.is_some() {
                        hovered_message = hovered;
                    }
                    if i == stop {
                        crate::a11y::list_stop(&resp, rows_id, self.selected == Some(i));
                    }
                    if resp.clicked() {
                        // Ctrl+click: compare the selected commit with this one.
                        let compare = ui
                            .input(|inp| inp.modifiers.command)
                            .then(|| self.compare_rows(self.selected?, i))
                            .flatten();
                        acts.push(compare.unwrap_or(Action::Select(i)));
                    }
                    if resp.secondary_clicked() {
                        let hash = self.commits[i].hash.clone();
                        ui.data_mut(|d| d.insert_temp(menu_row_id(), hash));
                    }
                    self.row_menu(&resp, i, idle, acts);
                    continue;
                }
                // The ghost graph is only looked at: its rows do nothing, but the menu open
                // on a real commit (whose entry is being previewed) stays open.
                let resp = self.ghost_row(ui, i, cols, lanes, &readable);
                if !self.is_ghost_row(i) {
                    menu_drawn |= menu_row.as_deref() == Some(self.commits[i].hash.as_str());
                    self.row_menu(&resp, i, idle, acts);
                }
            }
            if ghost && !menu_drawn {
                self.keep_menu_open(ui, menu_row.as_deref(), idle, acts);
            }
        });
        ui.data_mut(|d| d.insert_temp(offset_id, out.state.offset.y));
        if show_map && !ghost {
            self.history_minimap(ui, map_rect, out.state.offset.y, out.inner_rect.height());
        }
        if let Some(steps) = first_steps {
            self.first_steps_view(ui, steps, acts);
        }
        self.want_remerge_badges(&shown_merges, ui.ctx());
        let graph_label = crate::repo::graph_order::header_label(self.log_order);
        header::column_headers(ui, &t, header, &cols, &graph_label);
        if self.test_lane.visible {
            self.test_lane_header(ui, header, &cols, acts);
        }
        self.column_handles(ui, header, &prefs, &cols, &settings);
        header::header_menu(&header_resp, &settings);
        order::order_button(
            ui,
            header,
            &cols,
            (&graph_label, self.log_order),
            &settings,
            acts,
        );
        self.hover_graph_message(
            ui.ctx(),
            if ghost {
                None
            } else {
                hovered_message.as_deref()
            },
        );
    }

    /// A row of the ghost graph, faded as what it is: a commit the operation would make, one
    /// it would leave on no ref, or one as it is.
    fn ghost_row(
        &self,
        ui: &mut Ui,
        i: usize,
        cols: Cols,
        lanes: Lanes,
        viewport: &readability::Viewport,
    ) -> Response {
        let fade = match self.ghost.kind(i) {
            Kind::New => GHOST_OPACITY,
            Kind::Going => GOING_OPACITY,
            Kind::Real => 1.0,
        };
        let before = ui.opacity();
        ui.multiply_opacity(fade);
        // What its pills and build dots would do is dropped: nothing here is real yet.
        let resp = self
            .commit_row(ui, i, cols, lanes, viewport, false, &mut Vec::new())
            .0;
        ui.set_opacity(before);
        resp
    }

    /// The menu of the commit right-clicked last when its row has scrolled out of the ghost
    /// graph's view (the rows it adds push it down): drawn without its row, so it stays open.
    fn keep_menu_open(&self, ui: &mut Ui, hash: Option<&str>, idle: bool, acts: &mut Vec<Action>) {
        let Some(i) = hash.and_then(|h| self.index.get(h)).copied() else {
            return;
        };
        let at = egui::Rect::from_min_size(ui.min_rect().min, vec2(0.0, 0.0));
        let resp = ui.interact(at, row::row_id(ui, &self.commits[i].hash), Sense::click());
        self.row_menu(&resp, i, idle, acts);
    }

    /// Up and down select the row above or below the selected one.
    /// `ringed`: the row Tab reaches, where they start while nothing is selected.
    fn arrow_keys(&mut self, ui: &Ui, ringed: Option<usize>, acts: &mut Vec<Action>) {
        if Settings::get(ui.ctx()).graph_lane_walk {
            let direction = ui.input(|i| {
                if !i.modifiers.is_none() {
                    return None;
                }
                use crate::repo::graph_walk::Direction;
                if i.key_pressed(Key::ArrowDown) {
                    Some(Direction::Parent)
                } else if i.key_pressed(Key::ArrowUp) {
                    Some(Direction::Child)
                } else if i.key_pressed(Key::ArrowRight) {
                    Some(Direction::Merge)
                } else if i.key_pressed(Key::ArrowLeft) {
                    Some(Direction::Tip)
                } else {
                    None
                }
            });
            if let Some(direction) = direction
                && let Some(next) = self.graph_walk(direction, ringed, ui.ctx())
            {
                acts.push(Action::Select(next));
                self.scroll_to = Some(next);
            }
            return;
        }
        // Alt+↑ / Alt+↓ go from change to change of the diff shown below the graph.
        let below = self.diff_beside_graph(ui.ctx());
        let (up, down) = ui.input(|i| {
            let plain = !(below && i.modifiers.alt);
            (
                plain && i.key_pressed(Key::ArrowUp),
                plain && i.key_pressed(Key::ArrowDown),
            )
        });
        let current = self.selected.or(ringed).unwrap_or(0);
        let next = if up {
            Some(current.saturating_sub(1))
        } else if down {
            Some((current + 1).min(self.commits.len().saturating_sub(1)))
        } else {
            None
        };
        if let Some(next) = next {
            acts.push(Action::Select(next));
            self.scroll_to = Some(next);
        }
    }

    /// Right-click menu of row `i`: the changes row's, a stash's or a commit's.
    fn row_menu(&self, resp: &Response, i: usize, idle: bool, acts: &mut Vec<Action>) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        if commit.hash == WIP_HASH {
            wip_menu(resp, idle, acts);
            return;
        }
        if let Some(stash) = self.stash_at(i).and_then(|s| self.stashes.get(s)) {
            egui::Popup::context_menu(resp)
                .style(crate::theme::menu_style)
                .show(|ui| super::sidebar::stash_menu(ui, acts, stash, idle));
            return;
        }
        self.commit_menu(resp, i, idle, acts);
    }

    /// Compares two rows of the graph, the older one first; the changes row stands for the
    /// working tree. `None` for the same row twice.
    fn compare_rows(&self, a: usize, b: usize) -> Option<Action> {
        if a == b {
            return None;
        }
        // Rows go newest first.
        let (old, new) = (a.max(b), a.min(b));
        let end = |row: usize| {
            let c = self.commits.get(row)?;
            let label = badges(&c.refs, &self.refs)
                .into_iter()
                .find(|b| b.kind != BadgeKind::DetachedHead)
                .map(|b| b.label)
                .unwrap_or_else(|| super::short(&c.hash).to_owned());
            Some((c.hash.clone(), label))
        };
        let (base, base_label) = end(old)?;
        let (target, target_label) = if self.is_wip(new) {
            (None, "working tree".to_owned())
        } else {
            let (hash, label) = end(new)?;
            (Some(hash), label)
        };
        Some(Action::Compare {
            base,
            base_label,
            target,
            target_label,
            since_fork: false,
        })
    }
}

/// Where the graph remembers the commit whose menu was opened last.
fn menu_row_id() -> egui::Id {
    egui::Id::new("graph-menu-row")
}

/// The lanes to draw, from the settings and the graph's `width` in lanes.
fn lane_layout(settings: &Settings, width: usize) -> Lanes {
    Lanes {
        shown: width.clamp(1, MAX_LANES_SHOWN),
        width: if settings.compact_graph {
            COMPACT_LANE_W
        } else {
            LANE_W
        },
    }
}

/// The columns' preferred widths, from the settings; the graph fits its `lanes` unless its
/// width was dragged.
fn preferred_cols(settings: &Settings, lanes: Lanes) -> Cols {
    let shown_if = |shown: bool, width: f32| if shown { width.max(MIN_COL_W) } else { 0.0 };
    Cols {
        refs: settings.refs_width.max(MIN_REFS_W),
        graph: if settings.graph_width > 0.0 {
            settings.graph_width.max(MIN_GRAPH_W)
        } else {
            (lanes.shown as f32 * lanes.width + 12.0).max(MIN_GRAPH_W)
        },
        hash: shown_if(settings.show_hash, settings.hash_width),
        author: shown_if(settings.show_author, settings.author_width),
        when: shown_if(settings.show_when, settings.when_width),
        date: shown_if(settings.show_date, settings.date_width),
        push: settings.column_push,
    }
}

/// The last row while older history is still loading.
fn loading_row(ui: &mut Ui, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_PITCH), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Loading older commits…",
        FontId::proportional(12.0),
        t.text_faint,
    );
}

/// Right-click menu of the changes row.
fn wip_menu(resp: &Response, idle: bool, acts: &mut Vec<Action>) {
    egui::Popup::context_menu(resp)
        .style(crate::theme::menu_style)
        .show(|ui| {
            if MenuItem::new("Stage all changes")
                .enabled(idle)
                .keys(Command::StageAll)
                .show(ui)
                .clicked()
            {
                acts.push(Action::Ops {
                    label: "Stage all".into(),
                    ops: ops(&[&["add", "-A"]]),
                    refresh: Refresh::Status,
                });
            }
            if MenuItem::new("Stash changes…")
                .enabled(idle)
                .keys(Command::Stash)
                .show(ui)
                .clicked()
            {
                acts.push(Action::Dialog(Dialog::stash()));
            }
            ui.separator();
            if ui
                .add_enabled(idle, egui::Button::new("Apply patch…"))
                .on_hover_text(APPLY_PATCH_TIP)
                .clicked()
            {
                acts.push(Action::ApplyPatch);
            }
        });
}

/// What "Apply patch…" does.
const APPLY_PATCH_TIP: &str = "A patch from git format-patch becomes commits (git am), any other diff goes on the working \
     tree (git apply)";

/// Menu entries comparing `branch` with `current`: what it would bring in (since they
/// diverged, as a pull request shows it), and every difference.
pub(super) fn compare_items(
    ui: &mut Ui,
    acts: &mut Vec<Action>,
    branch: BranchRef<'_>,
    current: &str,
) {
    let name = branch.name;
    let current_ref = BranchRef::local(current);
    if ui
        .button(format!("Changes {name} adds to {current}"))
        .on_hover_text("What it changed since the two diverged, as a pull request shows it")
        .clicked()
    {
        acts.push(compare_branches_action(current_ref, branch, true));
    }
    if ui
        .button(format!("Compare {current} with {name}"))
        .on_hover_text("Every difference between the two")
        .clicked()
    {
        acts.push(compare_branches_action(current_ref, branch, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;

    #[test]
    fn a_selected_pr_branch_is_visible_ahead_of_other_branches_at_its_tip() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-pill-focus".into(), &ctx);
        let commit = git::Commit {
            hash: "head".into(),
            refs: vec!["HEAD -> dev".into(), "feature".into()],
            ..git::Commit::default()
        };
        tab.compare = Some(crate::repo::Compare {
            base: "main".into(),
            base_label: "main".into(),
            target: Some("head".into()),
            target_label: "feature".into(),
            since_fork: true,
            data: None,
            request: 1,
        });
        let focused = tab.graph_badges(&commit);
        assert_eq!(focused[0].label, "feature");
        assert_eq!(focused[1].label, "dev");
        tab.prs.push(crate::forge::PullRequest {
            number: 7,
            title: String::new(),
            branch: "fork-feature".into(),
            base: "main".into(),
            sha: "head".into(),
            from_fork: true,
            author: String::new(),
            url: String::new(),
            draft: false,
        });
        if let Some(compare) = &mut tab.compare {
            compare.target_label = "fork-feature".into();
        }
        let focused = tab.graph_badges(&commit);
        assert_eq!(focused[0].label, "fork-feature");
        assert_eq!(focused[0].kind, BadgeKind::PullRequest);
        assert!(focused[0].full_refs().is_empty());
        assert!(!focused[0].local && focused[0].remotes.is_empty());
    }

    #[test]
    fn a_compact_graph_column_fits_its_lanes_closer_together() {
        let wide = Settings::default();
        let compact = Settings {
            compact_graph: true,
            ..Settings::default()
        };
        let graph = |s: &Settings| preferred_cols(s, lane_layout(s, 10)).graph;
        assert_eq!(graph(&wide), 172.0);
        assert_eq!(graph(&compact), 112.0);
        // A width dragged by hand stays.
        let dragged = Settings {
            graph_width: 150.0,
            ..compact
        };
        assert_eq!(graph(&dragged), 150.0);
    }

    /// The graph of `tab` showing the commits `hashes` (newest first, each on the next).
    fn show_commits(tab: &mut RepoTab, hashes: &[&str]) {
        tab.commits = hashes
            .iter()
            .enumerate()
            .map(|(i, h)| {
                std::sync::Arc::new(git::Commit {
                    hash: (*h).to_owned(),
                    parents: hashes
                        .get(i + 1)
                        .map(|p| (*p).to_owned())
                        .into_iter()
                        .collect(),
                    summary: format!("commit {h}"),
                    ..Default::default()
                })
            })
            .collect();
        tab.graph = crate::graph::layout(&tab.commits);
        tab.index = hashes
            .iter()
            .enumerate()
            .map(|(i, h)| ((*h).to_owned(), i))
            .collect();
    }

    /// A commit's menu stays open while a ghost run's rows push its row out of view (the
    /// entry under the pointer is what is previewed), and the ghost graph draws in its place.
    #[test]
    fn a_menu_stays_open_while_ghost_rows_push_its_row_away() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/ghost-menu-test".into(), &ctx);
        let hashes: Vec<String> = ["c", "b", "a"].iter().map(|h| h.repeat(40)).collect();
        let refs: Vec<&str> = hashes.iter().map(String::as_str).collect();
        show_commits(&mut tab, &refs);
        // A short window: the header and a few rows.
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 160.0));
        let second = egui::pos2(600.0, 26.0 + ROW_PITCH * 1.5);
        let frame = |tab: &mut RepoTab, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
            })
            .drop_without_applying_deltas();
        };
        frame(&mut tab, vec![egui::Event::PointerMoved(second)]);
        let click = |pressed| egui::Event::PointerButton {
            pos: second,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut tab, vec![click(true), click(false)]);
        frame(&mut tab, Vec::new());
        let opened = egui::Popup::is_any_open(&ctx);
        // Twenty commits on top: the right-clicked row is far below the view now.
        let created = (0..20)
            .map(|n| git::Commit {
                hash: format!("{n:040}"),
                parents: vec![if n == 19 {
                    hashes[0].clone()
                } else {
                    format!("{:040}", n + 1)
                }],
                summary: format!("copy {n}"),
                ..Default::default()
            })
            .collect();
        let outcome = crate::repo::ghost::Outcome {
            created,
            copies: 20,
            ..Default::default()
        };
        let rows = tab.commits.clone();
        tab.ghost.show_for_test(outcome, &rows);
        // Near the end of what is loaded, with more to load: the ghost graph asks for none.
        tab.has_more = true;
        tab.selected = Some(2);
        let mut asked = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| tab.graph_view(ui, &mut asked));
            })
            .drop_without_applying_deltas();
        }
        assert!(!asked.iter().any(|a| matches!(a, Action::LoadMore)));
        assert_eq!(tab.selected, Some(2));
        assert!(opened, "the right-click opened no menu");
        assert!(egui::Popup::is_any_open(&ctx), "the menu closed");
        // The tab's own rows are back once the graph is drawn.
        assert_eq!(tab.commits.len(), 3);
        assert!(!tab.ghost.drawing());
    }

    /// A commit's menu left open while a reload replaces the rows is that commit's: when the
    /// commit is gone (amended, or reset away in the terminal) it closes, rather than turning
    /// into the menu of the commit that took its row, whose Revert or Cherry-pick runs at once.
    #[test]
    fn a_commit_menu_does_not_move_to_the_commit_that_takes_its_row() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/row-menu-test".into(), &ctx);
        show_commits(
            &mut tab,
            &["c".repeat(40).as_str(), &"b".repeat(40), &"a".repeat(40)],
        );
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0));
        // The header, then the rows: the second row's middle, away from its pills.
        let second = egui::pos2(600.0, 26.0 + ROW_PITCH * 1.5);
        let frame = |tab: &mut RepoTab, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
            })
            .drop_without_applying_deltas();
        };
        frame(&mut tab, vec![egui::Event::PointerMoved(second)]);
        let click = |pressed| egui::Event::PointerButton {
            pos: second,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut tab, vec![click(true), click(false)]);
        frame(&mut tab, Vec::new());
        let opened = egui::Popup::is_any_open(&ctx);
        // The right-clicked commit is gone: another one is in its row now.
        show_commits(&mut tab, &["d".repeat(40).as_str(), &"a".repeat(40)]);
        frame(&mut tab, Vec::new());
        frame(&mut tab, Vec::new());
        assert!(opened, "the right-click opened no menu");
        assert!(!egui::Popup::is_any_open(&ctx));
    }
}
