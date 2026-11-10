//! Central pane: the commit graph of every branch, with branch pills on the left, search and
//! older history loaded as you scroll.

mod menus;
pub(super) use menus::compare_items;
use menus::*;

mod paint;
use paint::*;

mod interaction;

mod viewport;

mod badge;
mod drag;
mod handles;
mod header;
mod lanes;
#[cfg(test)]
mod layout_tests;
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

#[cfg(test)]
use egui::Key;
use egui::{Align2, FontId, Response, Sense, Ui, vec2};

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
use badge::{Badge, BadgeKind};
/// For the settings' previews, which draw a graph of their own like this one.
pub(crate) use lanes::edge;
use row::Lanes;
pub(in crate::repo) use row::pill;

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
const AVATAR_D: f32 = 20.0;

// Observe the geometry actually painted after readable_viewport's automatic refs adjustment.
// Tests drive graph_view itself; this hook never computes an alternative test-only layout.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct PaintedColumns {
    header: egui::Rect,
    prefs: Cols,
    cols: Cols,
}

#[cfg(test)]
fn painted_columns_id() -> egui::Id {
    egui::Id::new("history-painted-columns")
}
/// How opaque a commit a ghost run would make is drawn.
const GHOST_OPACITY: f32 = 0.6;
/// How opaque a commit it would leave on no ref is drawn.
const GOING_OPACITY: f32 = 0.4;

impl RepoTab {
    /// A comparison opened from a pull request focuses that branch, including when several
    /// branches share its tip and the ordinary ordering would put it behind `+N`.
    fn graph_badges(&self, commit: &git::Commit) -> Vec<Badge> {
        let mut badges = self.graph_badge_index.badges(&commit.refs);
        for badge in &mut badges {
            if badge.kind != BadgeKind::Branch {
                continue;
            }
            badge.remotes.retain(|remote| {
                self.visibility
                    .shows(&super::branch_visibility::remote_ref(&format!(
                        "{remote}/{}",
                        badge.label
                    )))
            });
            badge.local &= badge.head
                || self
                    .visibility
                    .shows(&super::branch_visibility::local_ref(&badge.label));
        }
        badges.retain(|b| b.kind != BadgeKind::Branch || b.local || !b.remotes.is_empty());
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
        // One row: the history filters, then which part of the history the graph shows.
        let saved_views = self.timeline_view.is_none();
        ui.horizontal_wrapped(|ui| {
            self.history_filter_controls(ui);
            if saved_views {
                ui.separator();
                self.saved_graph_controls(ui);
            }
        });
        if saved_views {
            self.saved_graph_status(ui);
        }
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

    // The menu of the commit right-clicked last when its row has scrolled out of the ghost
    // graph's view (the rows it adds push it down): drawn without its row, so it stays open.
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

#[cfg(test)]
#[path = "graph_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "graph_view_registry_tests.rs"]
mod registry_tests;

#[cfg(test)]
#[path = "graph_view_remote_visibility_tests.rs"]
mod remote_visibility_tests;
