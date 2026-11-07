//! Central pane: the commit graph of every branch, with branch pills on the left, search and
//! older history loaded as you scroll.

mod badge;
mod handles;
mod header;
mod lanes;
mod menu;
pub(super) mod message_tooltip;
mod pills;
mod readability;
#[cfg(test)]
mod readability_tests;
mod row;
mod search;

use egui::{Align2, FontId, Key, Response, Sense, Ui, vec2};

use super::actions::{BranchRef, compare_branches_action};
use super::columns::{Cols, MIN_COL_W, MIN_GRAPH_W, MIN_REFS_W};
use super::{Action, Dialog, Refresh, RepoTab, WIP_HASH, ops};
use crate::settings::Settings;
use crate::theme::Tokens;
use badge::{BadgeKind, badges};
/// For the settings' previews, which draw a graph of their own like this one.
pub(crate) use lanes::edge;
use row::Lanes;

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

impl RepoTab {
    pub(super) fn graph_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
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
        self.graph_motion.expire();
        self.saved_graph_header(ui);
        self.graph_walk_header(ui);
        if self.commits.is_empty() {
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
        let graph_row_focused = self
            .graph_walk_focus_id
            .is_some_and(|id| ui.ctx().memory(|m| m.focused()) == Some(id));
        if (!ui.ctx().egui_wants_keyboard_input() || graph_row_focused)
            && self.dialog.is_none()
            && self.ssh_setup.is_none()
            && !self.keys_blocked
            && self.keyboard_graph_active()
            && !egui::Popup::is_any_open(ui.ctx())
        {
            self.arrow_keys(ui, acts);
        }

        let settings = Settings::get(ui.ctx());
        let lanes = lane_layout(&settings, self.graph.width);
        let mut prefs = preferred_cols(&settings, lanes);
        let mut cols = prefs.squeezed(ui.available_width(), settings.last_resized);

        // Column headers.
        let (header, header_resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        // Paint the header after the viewport has supplied this frame's ref width.

        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("graph_scroll")
            .auto_shrink(false);
        let offset_id = ui.id().with("graph_offset");
        if let Some(row) = self.scroll_to.take() {
            let view_h = ui.available_height();
            let row = self.saved_views.position(row).unwrap_or(row);
            let top = row as f32 * ROW_PITCH;
            // Only scroll if the row is outside the visible part.
            let offset = ui.data(|d| d.get_temp::<f32>(offset_id)).unwrap_or(0.0);
            if top < offset || top + ROW_PITCH > offset + view_h {
                let target = (top - view_h / 2.0).max(0.0);
                scroll = scroll.vertical_scroll_offset(self.graph_motion.start_scroll(
                    offset,
                    target,
                    ui.ctx(),
                ));
            }
        }

        if ui.input(|i| i.smooth_scroll_delta.y != 0.0 || i.pointer.any_pressed()) {
            self.graph_motion.cancel_scroll();
        }
        if let Some(offset) = self.graph_motion.scroll_offset(ui.ctx()) {
            scroll = scroll.vertical_scroll_offset(offset);
        }

        // `show_rows` sizes rows as ROW_PITCH + item spacing, so the spacing must be zero here.
        ui.spacing_mut().item_spacing.y = 0.0;
        let idle = self.busy_op.is_none();
        let filtered = self.saved_views.active();
        let total = if filtered {
            self.saved_views.len()
        } else {
            self.commits.len() + usize::from(self.has_more)
        };
        let mut hovered_message = None;
        let mut mounted = std::collections::HashMap::new();
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
            let readable = if filtered {
                self.readable_rows(
                    ui,
                    (first..end).filter_map(|i| match self.saved_views.row(i) {
                        Some(super::saved_views::Row::Commit(raw)) => Some(*raw),
                        _ => None,
                    }),
                    lanes.shown,
                )
            } else {
                self.readable_viewport(ui, first..end, lanes.shown)
            };
            if settings.fit_visible_refs() {
                prefs.refs = readable.refs_width;
            }
            cols = prefs.squeezed(ui.available_width(), settings.last_resized);
            // Near the end: fetch the next page of history.
            if !filtered
                && range.end + 40 >= self.commits.len()
                && self.has_more
                && !self.loading_more
            {
                acts.push(Action::LoadMore);
            }
            for projected in range {
                let i = if filtered {
                    match self.saved_views.row(projected) {
                        Some(super::saved_views::Row::Commit(raw)) => *raw,
                        Some(super::saved_views::Row::Gap { range, top, bottom }) => {
                            if connector_row(ui, &t, cols, lanes, range.len(), top, bottom)
                                .clicked()
                            {
                                self.expand_graph_span(range.clone());
                            }
                            continue;
                        }
                        None => continue,
                    }
                } else {
                    projected
                };
                if i >= self.commits.len() {
                    loading_row(ui, &t);
                    continue;
                }
                let (resp, hovered) = self.commit_row(ui, i, cols, lanes, &readable, acts);
                mounted.insert(i, resp.id);
                if hovered.is_some() {
                    hovered_message = hovered;
                }
                if resp.clicked() {
                    // Ctrl+click: compare the selected commit with this one.
                    let compare = ui
                        .input(|inp| inp.modifiers.command)
                        .then(|| self.compare_rows(self.selected?, i))
                        .flatten();
                    acts.push(compare.unwrap_or(Action::Select(i)));
                }
                self.row_menu(&resp, i, idle, acts);
            }
        });
        for (&raw, &id) in &mounted {
            ui.ctx().accesskit_node_builder(id, |node| {
                node.set_description(self.graph_description(raw));
                let parents: Vec<_> = self
                    .topology
                    .parents
                    .get(raw)
                    .into_iter()
                    .flatten()
                    .filter_map(|parent| mounted.get(parent).map(egui::Id::accesskit_id))
                    .collect();
                node.set_flow_to(parents);
            });
        }
        ui.data_mut(|d| d.insert_temp(offset_id, out.state.offset.y));
        header::column_headers(ui, &t, header, &cols);
        self.column_handles(ui, header, &prefs, &cols, &settings);
        header::header_menu(&header_resp, &settings);
        self.hover_graph_message(ui.ctx(), hovered_message.as_deref());
    }

    /// Up and down select the row above or below the selected one.
    fn arrow_keys(&mut self, ui: &Ui, acts: &mut Vec<Action>) {
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
                && let Some(next) = self.graph_walk(direction, ui.ctx())
            {
                acts.push(Action::Select(next));
                self.scroll_to = Some(next);
            }
            return;
        }
        let (up, down) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
        let current = self.selected.unwrap_or(0);
        let next = if self.saved_views.active() && (up || down) {
            self.saved_views.next(current, down)
        } else if up {
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
            if ui.button("Earlier versions…").clicked() {
                acts.push(Action::EarlierVersions);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Stage all changes"))
                .clicked()
            {
                acts.push(Action::Ops {
                    label: "Stage all".into(),
                    ops: ops(&[&["add", "-A"]]),
                    refresh: Refresh::Status,
                });
            }
            if ui
                .add_enabled(idle, egui::Button::new("Stash changes…"))
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

/// A collapsed run preserves only continuous lanes at its boundaries. Ended/reused lanes
/// have separate caps; hidden internal merges are deliberately not invented as edges.
fn connector_row(
    ui: &mut Ui,
    t: &Tokens,
    cols: Cols,
    lanes: Lanes,
    count: usize,
    top: &[Option<usize>; 24],
    bottom: &[Option<usize>; 24],
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_PITCH), Sense::click());
    let painter = ui.painter_at(rect);
    let mid = rect.center().y;
    for col in 0..lanes.shown.min(24) {
        let x = rect.left() + cols.refs + 6.0 + col as f32 * lanes.width + lanes.width / 2.0;
        let stroke = egui::Stroke::new(1.3, t.lane(col).gamma_multiply(0.65));
        let continuous = top[col].is_some() && top[col] == bottom[col];
        for step in 0..6 {
            let y = rect.top() + step as f32 * 5.0;
            let upper = y < mid - 3.0;
            let lower = y > mid + 1.0;
            if continuous || (upper && top[col].is_some()) || (lower && bottom[col].is_some()) {
                painter.line_segment(
                    [
                        egui::pos2(x, y),
                        egui::pos2(x, (y + 3.0).min(rect.bottom())),
                    ],
                    stroke,
                );
            }
        }
        if !continuous {
            if top[col].is_some() {
                painter.line_segment(
                    [
                        egui::pos2(x - 2.0, mid - 3.0),
                        egui::pos2(x + 2.0, mid - 3.0),
                    ],
                    stroke,
                );
            }
            if bottom[col].is_some() {
                painter.line_segment(
                    [
                        egui::pos2(x - 2.0, mid + 3.0),
                        egui::pos2(x + 2.0, mid + 3.0),
                    ],
                    stroke,
                );
            }
        }
    }
    painter.text(
        egui::pos2(rect.left() + cols.refs + cols.graph + 6.0, mid),
        Align2::LEFT_CENTER,
        format!("⋯ {count} intervening commits · expand"),
        FontId::proportional(12.0),
        if response.hovered() {
            t.text
        } else {
            t.text_dim
        },
    );
    response.on_hover_text("Expand this omitted history. Dashed lanes show continuous boundary runs; internal merges remain hidden until expanded.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;

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
