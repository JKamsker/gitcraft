//! The timeline in the big area (Ctrl+Shift+Z): the undo tree on the left, newest on top, the
//! entry picked on the right with what it changed and the ways back.

mod detail;
mod tree_rows;

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
            self.status = Some(crate::repo::Notice::plain((
                "Finish or close the conflict or the rebase editor first to see the timeline"
                    .to_owned(),
                true,
            )));
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
        widgets::panel_header(
            ui,
            crate::i18n::text("repo-workflows-timeline-timeline-view-ui-9dcff98e"),
            |ui| {
                if icons_button(
                    ui,
                    "x",
                    crate::i18n::text("repo-workflows-timeline-timeline-view-ui-961b2a04"),
                ) {
                    acts.push(Action::ToggleTimeline);
                }
                ui.menu_button(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-timeline-timeline-view-ui-8225e6ab",
                    ))
                    .size(12.0),
                    |ui| {
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-timeline-timeline-view-ui-3b4f9f54",
                            ))
                            .clicked()
                        {
                            acts.push(Action::Timeline(TimelineAct::AskForget(50)));
                        }
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-timeline-timeline-view-ui-9f54c719",
                            ))
                            .clicked()
                        {
                            acts.push(Action::Timeline(TimelineAct::AskForget(0)));
                        }
                    },
                );
                let cmd = crate::manual::command();
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-timeline-entry-count",
                        &[("count", (count).into()), ("command", (cmd).into())],
                    ))
                    .size(11.5)
                    .color(t.text_faint),
                );
            },
        );
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
                    RichText::new(crate::i18n::text(
                        "repo-workflows-timeline-timeline-view-ui-1d9ae2c2",
                    ))
                    .font(theme::semibold(15.0))
                    .color(t.text),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-timeline-timeline-view-ui-232c6bba",
                    ))
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
        if self.timeline_ghost_active() {
            egui::Panel::bottom("timeline_ghost_graph")
                .resizable(true)
                .default_size(280.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-timeline-timeline-view-ui-d4512de3",
                        ))
                        .color(t.text_dim),
                    );
                    ui.add_enabled_ui(false, |ui| self.graph_view(ui, &mut Vec::new()));
                });
        }
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
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::same(10))
            .outer_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-timeline-forget-confirm",
                        &[("count", (gone).into())],
                    ))
                    .color(t.text),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(
                        ui,
                        crate::i18n::text("repo-workflows-timeline-forget-confirm-a6bd489d"),
                        gone > 0,
                        80.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::Forget(keep)));
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-timeline-forget-confirm-19766ed6",
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::CancelForget));
                    }
                });
            });
    }
}

/// The files the entry changed.
fn changed_files(ui: &mut Ui, entry: &Entry) {
    let t = Tokens::get(ui.ctx());
    section(
        ui,
        crate::i18n::text("repo-workflows-timeline-changed-files-abc7e989"),
    );
    if entry.files.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-timeline-changed-files-88ffdf28",
            ))
            .color(t.text_faint),
        );
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
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-timeline-changed-files-e1614a83",
                &[("more", (more).into())],
            ))
            .color(t.text_faint),
        );
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
