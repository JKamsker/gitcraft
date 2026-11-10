//! The working-tree changes: conflicted, unstaged and staged files, staged and unstaged a file
//! or all at once.

mod headers;
use headers::*;

mod actions;
use actions::*;
pub(in crate::repo) use actions::{discard_dialog, stage_action};

mod rows;

use egui::{Align2, RichText, Sense, Ui, pos2, vec2};

use super::stash_picks::{mark_picked, stash_menu_items};
use crate::git::{Area, InProgress, StatusFile};
use crate::icons;
use crate::repo::diff::{literal_path, unstage_args};
use crate::repo::partial_stash::row_problem;
use crate::repo::widgets::{self, RowExtra};
use crate::repo::{Action, Dialog, DiffKey, Refresh, RepoTab, branch_tree, folding, ops};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn changes_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        widgets::panel_header(
            ui,
            crate::i18n::text("repo-core-0811"),
            branch_tree::files_toggle,
        );

        ui.horizontal_wrapped(|ui| {
            if ui
                .button(crate::i18n::text("repo-core-0812"))
                .on_hover_text(crate::i18n::text("repo-core-0813"))
                .clicked()
            {
                acts.push(Action::EarlierVersions);
            }
            self.review_all_button(
                ui,
                crate::repo::review_workflow::ReviewSource::Working,
                acts,
            );
        });
        // Keep both staging and composition reachable after reopening in a short window.
        // Always use the same scroll container, so resizing preserves editor/scroll state.
        let frame = egui::Frame::new()
            .fill(t.card)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .stroke(egui::Stroke::new(1.0, t.separator));
        let content_id = ui.id().with("commit_box_content_height");
        let maximum_height = (ui.available_height() * 0.65).max(1.0);
        // A ScrollArea doesn't grow its parent panel intrinsically. Give the panel the
        // measured content height, bounded by the space left for staging. exact_size also
        // overrides a small persisted panel size after shrinking and expanding the window.
        let margins = frame.total_margin().sum().y + 1.0; // Panel separator
        let natural_height = ui
            .data(|d| d.get_temp::<f32>(content_id))
            .map_or(maximum_height, |height| height + margins);
        let composer_height = natural_height.min(maximum_height);
        egui::Panel::bottom("commit_box")
            .exact_size(composer_height)
            .frame(frame)
            .show(ui, |ui| {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("commit_box_scroll")
                    .max_height((composer_height - margins).max(1.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.commit_box(ui, acts));
                ui.data_mut(|d| d.insert_temp(content_id, scroll.content_size.y));
            });

        self.keyboard_work_files(ui, acts);
        egui::ScrollArea::vertical()
            .id_salt("changes_scroll")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(state) = self.work.in_progress {
                    self.in_progress_banner(ui, acts, state);
                }
                self.stash_picks_bar(ui, acts);
                self.lock_banners(ui, acts);
                self.unity_banners(ui, acts);
                if !self.work.conflicted.is_empty() {
                    self.conflicted_list(ui, acts);
                }
                self.agent_turns(ui, acts);
                self.unstaged_list(ui, acts);
                self.staged_list(ui, acts);
                self.cochange_hints(ui, acts);
            });
    }

    /// The merge, rebase or the like going on, with Abort and what is left to do.
    fn in_progress_banner(&self, ui: &mut Ui, acts: &mut Vec<Action>, state: InProgress) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        egui::Frame::new()
            .fill(t.warning.gamma_multiply(0.14))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // A restack's rebase: continued and aborted as the restack, not on its own.
                let restack = self
                    .stacks
                    .restack_here()
                    .filter(|_| state == InProgress::Rebase);
                ui.horizontal(|ui| {
                    icons::inline(ui, "circle-alert", 16.0, t.warning);
                    // A fold stopped on a conflict says so: its changes are in the rebase.
                    let what = match self.fold_stopped_here(state) {
                        Some(fold) => crate::i18n::message(
                            "repo-core-0817",
                            &[(
                                "label",
                                crate::repo::messages::display_operation(&fold.label).into(),
                            )],
                        ),
                        None => crate::i18n::message(
                            "repo-core-0818",
                            &[("label", state.label().to_string().into())],
                        ),
                    };
                    let title = match restack {
                        Some(p) => crate::i18n::message(
                            "repo-core-0819",
                            &[
                                (
                                    "p",
                                    p.current()
                                        .map_or("", |s| s.branch.as_str())
                                        .to_string()
                                        .into(),
                                ),
                                ("done", format!("{}", p.done + 1).into()),
                                ("count", (p.steps.len()).into()),
                            ],
                        ),
                        None => crate::i18n::message(
                            "repo-core-0818",
                            &[("label", state.label().to_string().into())],
                        ),
                    };
                    ui.label(
                        RichText::new(if restack.is_some() { title } else { what })
                            .color(t.warning)
                            .font(theme::medium(12.5)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if restack.is_some() {
                            if ui
                                .add_enabled(
                                    idle,
                                    egui::Button::new(crate::i18n::text("repo-core-0820")),
                                )
                                .on_hover_text(crate::i18n::text("repo-core-0821"))
                                .clicked()
                            {
                                acts.push(Action::Stack(
                                    crate::repo::stack::StackAct::AbortRestack,
                                ));
                            }
                        } else if ui
                            .add_enabled(
                                idle,
                                egui::Button::new(crate::i18n::text("repo-core-0018")),
                            )
                            .clicked()
                        {
                            acts.push(self.abort_for(state));
                        }
                        if !self.work.conflicted.is_empty() {
                            self.resolve_all_button(ui, acts);
                        }
                    });
                });
                let hint = if restack.is_some() {
                    if self.work.conflicted.is_empty() {
                        crate::i18n::text("repo-core-0822")
                    } else {
                        crate::i18n::text("repo-core-0823")
                    }
                } else if self.work.conflicted.is_empty() && state == InProgress::Rebase {
                    crate::i18n::text("repo-core-0824")
                } else if self.work.conflicted.is_empty() {
                    crate::i18n::text("repo-core-0825")
                } else {
                    crate::i18n::text("repo-core-0826")
                };
                ui.label(RichText::new(hint).color(t.text_dim).small());
            });
    }

    fn conflicted_list(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        list_header(
            ui,
            crate::i18n::text("repo-core-0827"),
            self.work.conflicted.len(),
            None,
        );
        let buttons = [("check", crate::i18n::text("repo-core-0829"))];
        self.work_list(
            ui,
            acts,
            "conflicted",
            &self.work.conflicted,
            &buttons,
            |acts, f, click| match click {
                RowClick::Open => acts.push(Action::OpenDiff(DiffKey::Work(f.clone()))),
                RowClick::Button(_) => acts.push(stage_action(
                    crate::i18n::text("repo-core-0179"),
                    vec![vec!["--literal-pathspecs", "add", "--", &f.path]],
                )),
                RowClick::None | RowClick::Pick => {}
            },
        );
    }

    fn unstaged_list(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let unstaged = &self.work.unstaged;
        if list_header(
            ui,
            crate::i18n::text("repo-core-0831"),
            unstaged.len(),
            (!unstaged.is_empty() && idle).then_some(crate::i18n::text("repo-core-0832")),
        ) {
            acts.push(stage_action(
                crate::i18n::text("repo-core-0832"),
                vec![vec!["add", "-A"]],
            ));
        }
        let buttons = [
            ("undo-2", crate::i18n::text("repo-core-0834")),
            ("plus", crate::i18n::text("repo-core-0836")),
        ];
        self.work_list(
            ui,
            acts,
            "unstaged",
            unstaged,
            &buttons,
            |acts, f, click| match click {
                RowClick::Open => acts.push(Action::OpenDiff(DiffKey::Work(f.clone()))),
                RowClick::Button(0) => acts.push(Action::Dialog(discard_dialog(f))),
                RowClick::Button(_) => acts.push(stage_action(
                    crate::i18n::text("repo-core-0836"),
                    vec![vec!["--literal-pathspecs", "add", "--", &f.path]],
                )),
                RowClick::None | RowClick::Pick => {}
            },
        );
        if unstaged.is_empty() {
            empty_hint(ui, crate::i18n::text("repo-core-0838"));
        }
    }

    fn staged_list(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let head = &self.refs.head;
        let staged = &self.work.staged;
        if list_header_with(
            ui,
            crate::i18n::text("repo-core-0839"),
            staged.len(),
            (!staged.is_empty() && idle).then_some(crate::i18n::text("repo-core-0840")),
            |ui| {
                let reason = if !idle {
                    Some(crate::i18n::text("repo-core-1125"))
                } else if staged.is_empty() {
                    Some(crate::i18n::text("dev-integration-ui-stage-first"))
                } else if self.work.in_progress.is_some() {
                    Some(crate::i18n::text("dev-integration-ui-finish-operation"))
                } else if self.refs.current.is_none() {
                    Some(crate::i18n::text("repo-core-0842"))
                } else {
                    None
                };
                let r = ui
                    .add_enabled_ui(reason.is_none(), |ui| {
                        icons::small_button(
                            ui,
                            "git-branch-plus",
                            crate::i18n::text("repo-core-0049"),
                        )
                    })
                    .inner
                    .on_hover_text(crate::i18n::text("repo-core-0841"))
                    .on_disabled_hover_text(reason.unwrap_or_default());
                if r.clicked() {
                    acts.push(Action::OpenAbsorb);
                }
            },
        ) {
            acts.push(stage_action(
                crate::i18n::text("repo-core-0840"),
                vec![unstage_all_args(head)],
            ));
        }
        let paths: Vec<&str> = staged.iter().map(|f| f.path.as_str()).collect();
        self.owners_summary(ui, &paths);
        let buttons = [("minus", crate::i18n::text("repo-core-0844"))];
        self.work_list(
            ui,
            acts,
            "staged",
            staged,
            &buttons,
            |acts, f, click| match click {
                RowClick::Open => acts.push(Action::OpenDiff(DiffKey::Work(f.clone()))),
                RowClick::Button(_) => acts.push(Action::Ops {
                    label: "Unstage".to_owned(),
                    ops: vec![unstage_args(head, &f.path)],
                    refresh: Refresh::Status,
                }),
                RowClick::None | RowClick::Pick => {}
            },
        );
        if staged.is_empty() {
            empty_hint(ui, crate::i18n::text("repo-core-0846"));
        }
    }
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
