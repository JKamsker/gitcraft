//! The timeline in the big area (Ctrl+Shift+Z): the undo tree on the left, newest on top, the
//! entry picked on the right with what it changed and the ways back.

use egui::{Pos2, RichText, Stroke, Ui, pos2};

use super::{Entry, Origin, snapshot, tree};
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

const ROW_H: f32 = 26.0;
const LANE_W: f32 = 14.0;
/// Lanes drawn at most; further ones are drawn in the last.
const MAX_LANES: usize = 10;

/// What the timeline view asks for.
pub(crate) enum TimelineAct {
    /// Pick an entry (`None`: the state before them all).
    Select(Option<u64>),
    /// Ask before going to an entry's state, showing what it changes.
    AskJump(Option<u64>),
    CancelJump,
    Jump(Option<u64>),
    /// Put a branch back at a commit.
    PutBack {
        branch: String,
        commit: String,
    },
    /// Make a new branch at an old tip of `branch`.
    BranchAt {
        branch: String,
        commit: String,
    },
    /// Ask before forgetting all but the newest entries (all with 0).
    AskForget(usize),
    CancelForget,
    /// Forget all but the newest entries (all with 0).
    Forget(usize),
    /// Work out what removing an entry (only commits) takes out and plays again.
    AskRemove(u64),
    /// The new branch's name, as typed.
    RemoveName(String),
    /// Play the later commits again on the new branch.
    RunRemove,
    /// Ask before throwing away an attempt that stopped on a conflict (`false`: don't).
    AskDiscard(bool),
    /// Throw away an attempt that stopped on a conflict.
    DiscardRemove,
    CloseRemove,
}

/// The view's own state.
#[derive(Default)]
pub(crate) struct TimelineView {
    /// The row picked: `Some(None)` is the state before every entry.
    pub(crate) selected: Option<Option<u64>>,
    /// A jump asked for, waiting to be confirmed.
    pub(crate) confirm: Option<Option<u64>>,
    /// Forgetting asked for (how many to keep), waiting to be confirmed.
    pub(crate) forget: Option<usize>,
    /// Removing an entry while keeping later work, under way.
    pub(crate) remove: Option<super::remove_tab::RemoveState>,
}

impl RepoTab {
    /// Shows the timeline in the big area in place of a diff, a file, a pull request or builds,
    /// or closes it. Not over a conflict or the rebase editor, which come first.
    pub(in crate::repo) fn toggle_timeline(&mut self) {
        if self.timeline_view.take().is_some() {
            return;
        }
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some((
                "Finish or close the conflict or the rebase editor first to see the timeline"
                    .to_owned(),
                true,
            ));
            return;
        }
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        if self.pr_view.is_some() {
            self.close_pr();
        }
        self.tour = None;
        self.timeline_view = Some(TimelineView {
            selected: None,
            confirm: None,
            forget: None,
            remove: None,
        });
    }

    pub(in crate::repo) fn timeline_act(&mut self, act: TimelineAct, ctx: &egui::Context) {
        let Some(view) = &mut self.timeline_view else {
            return;
        };
        match act {
            TimelineAct::Select(id) => {
                view.selected = Some(id);
                view.confirm = None;
            }
            TimelineAct::AskJump(id) => view.confirm = Some(id),
            TimelineAct::CancelJump => view.confirm = None,
            TimelineAct::Jump(id) => {
                view.confirm = None;
                self.jump(id, ctx);
            }
            TimelineAct::PutBack { branch, commit } => self.put_branch_back(&branch, &commit, ctx),
            TimelineAct::BranchAt { branch, commit } => {
                self.branch_at_old_tip(&branch, &commit, ctx);
            }
            TimelineAct::AskRemove(id) => self.ask_remove(id, ctx),
            TimelineAct::RemoveName(name) => self.set_remove_name(name),
            TimelineAct::RunRemove => self.run_remove(ctx),
            TimelineAct::AskDiscard(ask) => self.confirm_discard(ask),
            TimelineAct::DiscardRemove => self.discard_remove(ctx),
            TimelineAct::CloseRemove => view.remove = None,
            TimelineAct::AskForget(keep) => view.forget = Some(keep),
            TimelineAct::CancelForget => view.forget = None,
            TimelineAct::Forget(keep) => {
                view.selected = None;
                view.forget = None;
                self.forget_timeline(keep, ctx);
            }
        }
    }

    pub(in crate::repo) fn timeline_view_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let count = self.timeline.entries.len();
        widgets::panel_header(ui, "Timeline", |ui| {
            if icons_button(ui, "x", "Close (Esc)") {
                acts.push(Action::ToggleTimeline);
            }
            ui.menu_button(RichText::new("Forget…").size(12.0), |ui| {
                if ui
                    .button("Forget all but the newest 50 (and the way to where you are)")
                    .clicked()
                {
                    acts.push(Action::Timeline(TimelineAct::AskForget(50)));
                }
                if ui.button("Forget the whole timeline").clicked() {
                    acts.push(Action::Timeline(TimelineAct::AskForget(0)));
                }
            });
            let cmd = crate::manual::command();
            ui.label(
                RichText::new(format!(
                    "{count} {} · {cmd}+Z / {cmd}+Y step back / forward",
                    if count == 1 { "entry" } else { "entries" }
                ))
                .size(11.5)
                .color(t.text_faint),
            );
        });
        if let Some(e) = &self.timeline_error {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(e).size(12.0).color(t.warning));
            });
        }
        if let Some(keep) = self.timeline_view.as_ref().and_then(|v| v.forget) {
            self.forget_confirm(ui, keep, acts);
        }
        if count == 0 {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Nothing in the timeline yet")
                        .font(theme::semibold(15.0))
                        .color(t.text),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Commits, merges, rebases, resets, checkouts and branch changes show up \
                         here, those made in gitcraft and those made elsewhere, and stay after a \
                         restart.",
                    )
                    .size(12.5)
                    .color(t.text_dim),
                );
            });
            return;
        }
        let selected = self
            .timeline_view
            .as_ref()
            .and_then(|v| v.selected)
            .unwrap_or(self.timeline.current);
        egui::Panel::left("timeline_tree")
            .resizable(true)
            .default_size(360.0)
            .size_range(220.0..=700.0)
            .frame(egui::Frame::new().fill(t.card))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.tree_rows(ui, selected, acts));
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(t.dock)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.entry_detail(ui, selected, idle, acts));
            });
    }

    /// Asks before forgetting: what can't be reached any more, and what doesn't change.
    fn forget_confirm(&self, ui: &mut Ui, keep: usize, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let path = self.timeline.path_to(self.timeline.current);
        let cut = self.timeline.entries.len().saturating_sub(keep);
        let gone = if keep == 0 {
            self.timeline.entries.len()
        } else {
            self.timeline.entries[..cut]
                .iter()
                .filter(|e| !path.contains(&e.id))
                .count()
        };
        let what = if gone == 1 { "entry" } else { "entries" };
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::same(10))
            .outer_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Forget {gone} {what}? Undo won't reach them any more, and the old commits only they kept can be removed by git gc. Your branches don't change."
                    ))
                    .color(t.text),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Forget", gone > 0, 80.0).clicked() {
                        acts.push(Action::Timeline(TimelineAct::Forget(keep)));
                    }
                    if ui.button("Cancel").clicked() {
                        acts.push(Action::Timeline(TimelineAct::CancelForget));
                    }
                });
            });
    }

    /// The tree, a row per entry, newest first.
    fn tree_rows(&self, ui: &mut Ui, selected: Option<u64>, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let (rows, lanes) = tree::layout(&self.timeline);
        let done = self.timeline.path_to(self.timeline.current);
        let lines = ui.painter().add(egui::Shape::Noop);
        let now = crate::git::now();
        let mut centers: Vec<Pos2> = Vec::with_capacity(rows.len());
        let shown_lanes = lanes.min(MAX_LANES);
        let graph_w = shown_lanes as f32 * LANE_W + 8.0;
        for row in &rows {
            let entry = row.id.and_then(|id| self.timeline.get(id));
            let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("timeline_row", row.id));
            if selected == row.id {
                ui.painter().rect_filled(rect, 0.0, t.row_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, t.hover);
            }
            let center = pos2(
                rect.left() + 8.0 + row.lane.min(MAX_LANES - 1) as f32 * LANE_W + LANE_W / 2.0,
                rect.center().y,
            );
            centers.push(center);
            let is_current = row.id == self.timeline.current;
            let on_path = row.id.is_none_or(|id| done.contains(&id));
            let color = t.lane(row.lane);
            if is_current {
                ui.painter().circle_filled(center, 5.5, color);
                ui.painter()
                    .circle_stroke(center, 8.0, Stroke::new(1.5, color));
            } else if on_path {
                ui.painter().circle_filled(center, 4.5, color);
            } else {
                ui.painter().circle_filled(center, 4.5, t.card);
                ui.painter()
                    .circle_stroke(center, 4.5, Stroke::new(1.5, color));
            }
            let text_x = rect.left() + 8.0 + graph_w;
            let (label, faint) = match entry {
                Some(e) => (e.label.clone(), !on_path),
                None => ("Before the oldest entry".to_owned(), false),
            };
            let label_color = if faint { t.text_faint } else { t.text };
            let when = entry.map(|e| crate::git::relative_time(e.time, now));
            let right = rect.right() - 8.0;
            let mut text_right = right;
            if let Some(when) = &when {
                let galley = ui.painter().layout_no_wrap(
                    when.clone(),
                    egui::FontId::proportional(11.0),
                    t.text_faint,
                );
                text_right -= galley.size().x + 8.0;
                ui.painter().galley(
                    pos2(
                        right - galley.size().x,
                        rect.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    t.text_faint,
                );
            }
            if entry.is_some_and(|e| e.origin == Origin::Outside) {
                let galley = ui.painter().layout_no_wrap(
                    "elsewhere".to_owned(),
                    theme::medium(10.5),
                    t.warning,
                );
                text_right -= galley.size().x + 8.0;
                ui.painter().galley(
                    pos2(text_right + 4.0, rect.center().y - galley.size().y / 2.0),
                    galley,
                    t.warning,
                );
            }
            let galley = ui.painter().layout(
                label,
                egui::FontId::proportional(12.5),
                label_color,
                (text_right - text_x).max(20.0),
            );
            let clipped = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                pos2(text_x, rect.top()),
                pos2(text_right, rect.bottom()),
            ));
            clipped.galley(pos2(text_x, rect.center().y - 8.0), galley, label_color);
            let resp = resp.on_hover_text(match entry {
                Some(e) => format!(
                    "{}\n{}",
                    e.label,
                    if e.origin == Origin::Outside {
                        "Made outside gitcraft (terminal, another app, or while it was closed)"
                    } else {
                        "Made in gitcraft"
                    }
                ),
                None => "The branches as they were before the oldest entry still in the timeline"
                    .to_owned(),
            });
            if resp.clicked() {
                acts.push(Action::Timeline(TimelineAct::Select(row.id)));
            }
            if resp.double_clicked() {
                acts.push(Action::Timeline(TimelineAct::AskJump(row.id)));
            }
        }
        // The lines, under the dots.
        let mut shapes = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let Some((parent_row, _)) = row.parent else {
                continue;
            };
            let (Some(from), Some(to)) = (centers.get(i), centers.get(parent_row)) else {
                continue;
            };
            let color = t.lane(row.lane).gamma_multiply(0.7);
            let bend = pos2(from.x, to.y - ROW_H / 2.0);
            shapes.push(egui::Shape::line_segment(
                [*from, bend],
                Stroke::new(1.5, color),
            ));
            shapes.push(egui::Shape::line_segment(
                [bend, *to],
                Stroke::new(1.5, color),
            ));
        }
        ui.painter().set(lines, egui::Shape::Vec(shapes));
    }

    /// The entry picked: what it changed, and the ways back to it.
    fn entry_detail(&self, ui: &mut Ui, id: Option<u64>, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let confirm = self.timeline_view.as_ref().and_then(|v| v.confirm);
        let Some(entry) = id.and_then(|id| self.timeline.get(id)) else {
            heading(ui, "Before the oldest entry");
            ui.label(
                RichText::new(
                    "The branches as they were before the oldest entry still in the timeline.",
                )
                .color(t.text_dim),
            );
            ui.add_space(10.0);
            self.jump_controls(ui, None, idle, confirm, acts);
            return;
        };
        heading(ui, &entry.label);
        let now = crate::git::now();
        let who = match entry.origin {
            Origin::Gitcraft => "Made in gitcraft",
            Origin::Outside => {
                "Made outside gitcraft (terminal, another app, or while gitcraft was closed)"
            }
        };
        ui.label(
            RichText::new(format!(
                "{who} · {}",
                crate::git::relative_time(entry.time, now)
            ))
            .size(12.0)
            .color(t.text_faint),
        );
        let state = if self.timeline.current == Some(entry.id) {
            ("The repository is here now", t.success)
        } else if self.timeline.is_done(entry.id) {
            ("Done: the repository is past this point", t.text_dim)
        } else {
            ("Undone, or on a side path of the tree", t.warning)
        };
        ui.label(RichText::new(state.0).size(12.0).color(state.1));
        ui.add_space(10.0);
        self.jump_controls(ui, Some(entry.id), idle, confirm, acts);
        ui.add_space(14.0);
        if let Some(receipt) = &entry.receipt {
            super::receipt_view::receipt_section(ui, receipt);
            ui.add_space(14.0);
        }
        self.changed_refs(ui, entry, idle, acts);
        ui.add_space(14.0);
        self.remove_section(ui, entry, idle, acts);
        ui.add_space(14.0);
        changed_files(ui, entry);
    }

    /// Go here (and, when asked, what it would change and the button that does it).
    fn jump_controls(
        &self,
        ui: &mut Ui,
        id: Option<u64>,
        idle: bool,
        confirm: Option<Option<u64>>,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let here = self.timeline.current == id;
        if confirm != Some(id) {
            let button = ui
                .add_enabled(idle && !here, egui::Button::new("Go here"))
                .on_hover_text(
                    "Put the branches and HEAD back as they were right after this (Double-click a \
                     row does too)",
                )
                .on_disabled_hover_text(if here {
                    "The repository is here already"
                } else {
                    "Wait until the current git operation finishes"
                });
            if button.clicked() {
                acts.push(Action::Timeline(TimelineAct::AskJump(id)));
            }
            return;
        }
        let Some((_, target)) = self.jump_target(id) else {
            return;
        };
        let now = self.ref_state();
        let lines = snapshot::describe(&now, &target);
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.label(
                        RichText::new(
                            "Nothing changes in the repository: only the marker in the timeline moves.",
                        )
                        .color(t.text_dim),
                    );
                } else {
                    ui.label(RichText::new("Going here changes:").color(t.text));
                    for line in &lines {
                        ui.label(
                            RichText::new(format!("•  {line}"))
                                .font(theme::mono(12.0))
                                .color(t.text_dim),
                        );
                    }
                    ui.label(
                        RichText::new(
                            "Changes in files are kept; staged ones end up unstaged, and git \
                             refuses where they'd be lost. Stashes and tags stay as they are.",
                        )
                        .size(11.5)
                        .color(t.text_faint),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Go here", idle, 90.0).clicked() {
                        acts.push(Action::Timeline(TimelineAct::Jump(id)));
                    }
                    if ui.button("Cancel").clicked() {
                        acts.push(Action::Timeline(TimelineAct::CancelJump));
                    }
                });
            });
    }

    /// The branches the entry made, moved or deleted, each with a way back to its old tip.
    fn changed_refs(&self, ui: &mut Ui, entry: &Entry, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let changes = entry.changes();
        section(ui, "Branches");
        if changes.is_empty() {
            ui.label(RichText::new("No branch moved (a stash, a tag…)").color(t.text_faint));
            return;
        }
        for change in changes {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(&change.name)
                        .font(theme::semibold(12.5))
                        .color(t.text),
                );
                ui.label(
                    RichText::new(format!(
                        "{}  →  {}",
                        ref_value(&change.old),
                        ref_value(&change.new)
                    ))
                    .font(theme::mono(12.0))
                    .color(t.text_dim),
                );
            });
            let is_branch = change.name != "HEAD";
            if is_branch && !change.old.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(12.0);
                    let back = format!("Put {} back at {}", change.name, short(&change.old));
                    if ui
                        .add_enabled(idle, egui::Button::new(back).small())
                        .on_hover_text(format!(
                            "Moves {} back to {} (resets it if it is checked out; uncommitted changes are kept). Undo undoes it.",
                            change.name,
                            short(&change.old)
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::PutBack {
                            branch: change.name.clone(),
                            commit: change.old.clone(),
                        }));
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("New branch there").small())
                        .on_hover_text(format!(
                            "Makes {} at {}, leaving {} as it is",
                            self.free_branch_name(&format!(
                                "{}-was-{}",
                                change.name,
                                short(&change.old)
                            )),
                            short(&change.old),
                            change.name
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::BranchAt {
                            branch: change.name.clone(),
                            commit: change.old.clone(),
                        }));
                    }
                    if ui
                        .small_button("Copy hash")
                        .on_hover_text(change.old.as_str())
                        .clicked()
                    {
                        ui.ctx().copy_text(change.old.clone());
                    }
                });
            }
            ui.add_space(4.0);
        }
    }
}

/// The files the entry changed.
fn changed_files(ui: &mut Ui, entry: &Entry) {
    let t = Tokens::get(ui.ctx());
    section(ui, "Files");
    if entry.files.is_empty() {
        ui.label(RichText::new("No file content changed").color(t.text_faint));
        return;
    }
    for file in &entry.files {
        ui.label(
            RichText::new(file)
                .font(theme::mono(12.0))
                .color(t.text_dim),
        );
    }
    let more = entry.files_total.saturating_sub(entry.files.len());
    if more > 0 {
        ui.label(RichText::new(format!("and {more} more")).color(t.text_faint));
    }
}

/// A branch or commit as the detail shows it: a commit by its short hash, nothing as “none”.
fn ref_value(value: &str) -> String {
    if value.is_empty() {
        "none".to_owned()
    } else if value.len() >= 40 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        short(value).to_owned()
    } else {
        value.to_owned()
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .font(theme::semibold(16.0))
                .color(t.text),
        )
        .wrap(),
    );
}

fn section(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.add_space(2.0);
}

/// An icon button in the header.
fn icons_button(ui: &mut Ui, icon: &str, tip: &str) -> bool {
    crate::icons::tool_button(ui, icon, "", true)
        .on_hover_text(tip)
        .clicked()
}
