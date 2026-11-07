//! The tab's layout: toolbar on top, the foldable side panels, the terminal and the central
//! view; and its keyboard shortcuts.

use egui::{Key, Ui};

use super::popout::Panel;
use super::{Action, RepoTab, pr_review, widgets};
use crate::a11y::Named;
use crate::app_buttons::{self, Place};
use crate::icons;
use crate::keymap;
use crate::settings::Settings;
use crate::theme::Tokens;

impl RepoTab {
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // Shown: its first read can't wait any more.
        self.start_deferred(&ctx);
        self.poll(&ctx);
        self.agents_frame(&ctx);
        self.resolver_frame(&ctx);
        let t = Tokens::get(&ctx);
        if self.opening() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.dock))
                .show(ui, |ui| self.opening_screen(ui));
            return;
        }
        self.publish_custom_commands(&ctx);
        let mut acts: Vec<Action> = std::mem::take(&mut self.queued);
        self.dco_frame(&ctx);
        self.owners_frame(&ctx);
        self.forks_frame(&ctx);
        self.hotfix_frame(&ctx);
        self.shortcuts(&ctx, &mut acts);
        self.drag_frame_start(&ctx);
        self.nav_shortcuts(&ctx, &mut acts);

        egui::Panel::top("repo_toolbar")
            .frame(
                egui::Frame::new()
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .stroke(egui::Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| self.toolbar(ui, &mut acts));
        self.push_banner(ui, &mut acts);
        self.detached_panel(ui, &mut acts);
        self.error_card_panel(ui);
        if self.running.as_ref().is_some_and(|r| r.network) {
            egui::Panel::top("repo_progress")
                .exact_size(3.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::new())
                .show(ui, |ui| self.progress_bar(ui));
        }
        self.side_panels(ui, &mut acts);
        self.tray_panel(ui, &mut acts);
        if self.activity.open {
            egui::Panel::bottom("repo_activity")
                .resizable(true)
                .default_size(240.0)
                .size_range(90.0..=900.0)
                .frame(
                    egui::Frame::new()
                        .fill(t.card)
                        .inner_margin(egui::Margin::symmetric(8, 6)),
                )
                .show(ui, |ui| self.activity_panel(ui, &mut acts));
        }
        if self.terminal_open && self.docked(Panel::Terminal) {
            egui::Panel::bottom("repo_terminal")
                .resizable(true)
                .default_size(260.0)
                .size_range(90.0..=900.0)
                .frame(egui::Frame::new().fill(t.card))
                .show(ui, |ui| self.terminal_panel(ui, &mut acts));
        }
        if self.test_lane.shows_banner() {
            egui::Panel::top("repo_tests")
                .frame(
                    egui::Frame::new()
                        .fill(t.chrome)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .stroke(egui::Stroke::new(1.0, t.separator)),
                )
                .show(ui, |ui| self.test_banner(ui, &mut acts));
        }
        if self.bisect.shows() {
            egui::Panel::top("repo_bisect")
                .frame(
                    egui::Frame::new()
                        .fill(t.chrome)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .stroke(egui::Stroke::new(1.0, t.separator)),
                )
                .show(ui, |ui| self.bisect_banner(ui, &mut acts));
        }
        if self.parallel.view().is_some() {
            egui::Panel::top("repo_parallel_bisect")
                .frame(
                    egui::Frame::new()
                        .fill(t.chrome)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .stroke(egui::Stroke::new(1.0, t.separator)),
                )
                .show(ui, |ui| self.parallel_banner(ui, &mut acts));
        }
        // Over the graph and the changes, not over the editors of a conflict or a rebase.
        if self.conflict.is_none() && self.rebase.is_none() {
            self.issue_card(ui, &mut acts);
        }
        let big_area = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.dock))
            .show(ui, |ui| {
                if self.conflict.is_some() {
                    self.conflict_view(ui, &mut acts);
                } else if self.rebase.is_some() {
                    self.rebase_view(ui, &mut acts);
                } else if self.split.is_some() {
                    self.graph_diff_view(ui, &mut acts);
                } else if self.absorb.is_some() {
                    self.absorb_view(ui, &mut acts);
                } else if self.diff_beside_graph(&ctx) {
                    self.split_view(ui, &mut acts);
                } else if self.diff.is_some()
                    && !self.history_beside_diff()
                    && self.docked(Panel::Diff)
                {
                    self.diff_view(ui, &mut acts);
                } else if self.file_view.is_some() {
                    self.file_view(ui, &mut acts);
                } else if self.ci_view.is_some() && self.docked(Panel::Ci) {
                    self.ci_view_ui(ui, &mut acts);
                } else if self.bakeoff.view.is_some() {
                    self.bakeoff_view(ui, &mut acts);
                } else if self.versions.view.is_some() {
                    self.versions_ui(ui, &mut acts);
                } else if self.pr_view.is_some() && self.docked(Panel::Pr) {
                    self.pr_view_ui(ui, &mut acts);
                } else if self.timeline_view.is_some() {
                    self.timeline_view_ui(ui, &mut acts);
                } else if self.doctor.is_some() {
                    self.doctor_ui(ui, &mut acts);
                } else {
                    if self.reflog.is_some() {
                        self.reflog_panel(ui, &mut acts);
                    }
                    self.graph_view(ui, &mut acts);
                }
            });

        self.resolver_card(&ctx, big_area.response.rect, &mut acts);
        self.drag_overlay(&ctx, &mut acts);
        self.dialogs(&ctx, &mut acts);
        self.keep_both_dialog(&ctx);
        self.ai_review_window(&ctx);
        self.lfs_locks_ui(&ctx, &mut acts);
        self.ghost_frame(&ctx);
        self.collision_window(&ctx, &mut acts);
        self.lfs_storage_ui(&ctx, &mut acts);
        self.issue_picker(&ctx, &mut acts);
        self.owners_lint_window(&ctx, &mut acts);
        self.fence_window(&ctx, &mut acts);
        self.unity_ui(&ctx, &mut acts);
        self.drift_window(&ctx, &mut acts);
        self.test_output_window(&ctx, &mut acts);
        self.rehearse_window(&ctx, &mut acts);
        self.interaction_window(&ctx, &mut acts);
        self.stack_dialogs(&ctx, &mut acts);
        self.agent_dialogs(&ctx, &mut acts);
        self.bakeoff_dialog(&ctx, &mut acts);
        self.release_dialog(&ctx, &mut acts);
        self.hotfix_dialog(&ctx, &mut acts);
        self.package_panel(&ctx, &mut acts);
        for action in acts {
            self.apply(action, &ctx);
        }
        self.record_view();
    }

    /// Until the first read is in: rather than branches, history and changes that all look
    /// empty, the repository being opened. Nothing at all for the first moments, so that a
    /// repository read in an instant doesn't flash it.
    fn opening_screen(&self, ui: &mut Ui) {
        const QUIET: std::time::Duration = std::time::Duration::from_millis(150);
        let waited = self.opened.elapsed();
        if waited < QUIET {
            ui.ctx().request_repaint_after(QUIET - waited);
            return;
        }
        let t = Tokens::get(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 70.0).max(0.0));
            icons::inline(ui, "folder-git-2", 40.0, t.text_faint);
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(format!("Opening {}", self.name))
                    .font(crate::theme::semibold(18.0))
                    .color(t.text),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(self.path.display().to_string())
                    .size(12.0)
                    .color(t.text_faint),
            );
            ui.add_space(18.0);
            ui.add(crate::spinner::Spinner::new().size(22.0));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Reading branches and history…")
                    .size(12.5)
                    .color(t.text_dim),
            );
        });
    }

    /// Escape closes the innermost view; Ctrl+Z / Ctrl+Y undo and redo, Ctrl+Shift+Z shows the
    /// undo tree, Ctrl+F searches, F7 and
    /// Shift+F7 (or Alt+↓ / Alt+↑) go to the next and previous change of a diff; the keys of
    /// [`crate::keymap`] fetch, pull, push, stash, branch, stage, commit and open the terminal.
    /// None of them while a dialog, a text field or the terminal has the keyboard.
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let free_keys = self.dialog.is_none()
            && self.package.is_none()
            && self.release.is_none()
            && self.hotfix.finish.is_none()
            && self.agents.dialog.is_none()
            && self.bakeoff.picker.is_none()
            && !self.stacks.modal_open()
            && !self.issue_picker_open()
            && !self.keys_blocked
            && !crate::a11y::keyboard_taken(ctx);
        // Ctrl+F, Esc and the steps of the find bar, when a diff, file or blame is shown.
        self.find_keys(ctx, free_keys);
        if free_keys {
            let commands = ctx.input(|i| keymap::pressed(i, keymap::Scope::Repository));
            // While a commit's message is being edited, Ctrl+Enter saves it (the editor takes
            // the key also when nothing has the focus): it doesn't commit the changes too.
            let rewording = self.reword.is_some();
            acts.extend(
                commands
                    .into_iter()
                    .filter(|&c| !(rewording && c == keymap::Command::Commit))
                    .map(Action::Key),
            );
            let (esc, undo, redo, tree, find) = ctx.input(|i| {
                (
                    i.key_pressed(Key::Escape),
                    i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z),
                    i.modifiers.command && i.key_pressed(Key::Y),
                    i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::Z),
                    // Not with Shift: Ctrl+Shift+F fetches.
                    i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::F),
                )
            });
            let diff_keys = self.diff.is_some()
                && (self.main_keys(ctx, Panel::Diff) || self.conflict.is_some());
            if esc && self.close_drop_menu() {
            } else if esc && diff_keys && self.note_open(ctx) {
                acts.push(Action::Agent(super::agents::AgentAct::Tray(
                    super::agents::TrayAct::Close,
                )));
            } else if esc && diff_keys && self.line_comment_open() {
                acts.push(Action::Pr(pr_review::PrAct::CloseLine));
            } else if esc && self.diff.as_ref().is_some_and(|d| d.moved.is_some()) {
                acts.push(Action::DiffMoved(None));
            } else if esc
                && self.conflict.is_none()
                && self.rebase.as_ref().is_some_and(|r| !r.typing)
            {
                // What is drawn first (see the central panel) closes first: a diff opened
                // meanwhile is under it.
                acts.push(Action::CloseRebase);
            } else if esc && self.conflict.is_none() && self.split.is_some() {
                acts.push(Action::CloseSplit);
            } else if esc && self.conflict.is_none() && self.absorb.is_some() {
                acts.push(Action::CloseAbsorb);
            } else if esc && diff_keys {
                acts.push(Action::CloseDiff);
            } else if esc && let Some(rebase) = &self.rebase {
                if !rebase.typing {
                    acts.push(Action::CloseRebase);
                }
            } else if esc && self.file_view.is_some() {
                acts.push(Action::CloseFileView);
            } else if esc && self.bakeoff.view.is_some() {
                acts.push(Action::Bake(super::bakeoff::BakeAct::Close));
            } else if esc && self.ci_view.is_some() && self.main_keys(ctx, Panel::Ci) {
                acts.push(Action::CloseCi);
            } else if esc && self.versions.view.is_some() {
                // The patch of a pair first, then the comparison.
                let open_pair = self
                    .versions
                    .view
                    .as_ref()
                    .is_some_and(|v| v.selected.is_some());
                acts.push(Action::Versions(if open_pair {
                    super::versions::VersionsAct::Deselect
                } else {
                    super::versions::VersionsAct::Close
                }));
            } else if esc && self.pr_view.is_some() && self.main_keys(ctx, Panel::Pr) {
                acts.push(Action::ClosePr);
            } else if esc && self.timeline_view.is_some() {
                acts.push(Action::ToggleTimeline);
            } else if esc && self.doctor.as_ref().is_some_and(|d| !d.asking()) {
                acts.push(Action::CloseDoctor);
            } else if esc && self.compare.is_some() {
                acts.push(Action::CloseCompare);
            } else if esc && self.search.open {
                acts.push(Action::ToggleSearch);
            } else if esc && self.reflog.is_some() {
                acts.push(Action::CloseReflog);
            }
            let diff_shown = diff_keys
                && self.conflict.is_none()
                && self.rebase.is_none()
                && self.split.is_none()
                && self.absorb.is_none();
            if let Some(delta) = ctx.input(change_step).filter(|_| diff_shown) {
                acts.push(Action::DiffStep(delta));
            }
            if undo {
                acts.push(Action::Undo);
            }
            if redo {
                acts.push(Action::Redo);
            }
            if tree {
                acts.push(Action::ToggleTimeline);
            }
            if find {
                self.search.open = true;
                self.search.focus = true;
            }
        } else if self.search.open
            && ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::F))
        {
            self.search.focus = true;
        }
    }

    /// The branches panel on the left and the detail panel on the right.
    fn side_panels(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let side = egui::Frame::new().fill(t.card);
        // Both side panels fold down to a strip holding their footer's toggle, also when an
        // edge is dragged past its minimum; dragging the strip out opens them again.
        let mut settings = Settings::get(&ctx);
        let (mut show_branches, mut show_detail) = (settings.show_branches, settings.show_detail);
        let area = ui.available_rect_before_wrap();
        egui::Panel::show_switched(
            ui,
            &mut show_branches,
            egui::Panel::left("repo_branches_folded")
                .resizable(true)
                .exact_size(widgets::FOLDED_W)
                .frame(side),
            egui::Panel::left("repo_branches")
                .resizable(true)
                .default_size(270.0)
                .size_range(170.0..=520.0)
                .frame(side),
            |ui, open| {
                widgets::panel_footer(ui, "repo_branches_footer", area.left(), false, |ui| {
                    if panel_toggle(ui, "panel-left", "branches", open) {
                        acts.push(Action::ToggleBranches);
                    }
                    if open {
                        app_buttons::footer_row(ui, Place::BottomLeft);
                    }
                });
                if open {
                    self.sidebar(ui, acts);
                } else {
                    let slid = widgets::slide_offset(ui, area.left(), false);
                    let below = app_buttons::folded_column(ui, Place::BottomLeft, slid);
                    self.folded_sidebar(ui, area.left(), below, acts);
                }
            },
        );
        egui::Panel::show_switched(
            ui,
            &mut show_detail,
            egui::Panel::right("repo_detail_folded")
                .resizable(true)
                .exact_size(widgets::FOLDED_W)
                .frame(side),
            egui::Panel::right("repo_detail")
                .resizable(true)
                .default_size(420.0)
                .size_range(280.0..=900.0)
                .frame(side),
            |ui, open| {
                widgets::panel_footer(ui, "repo_detail_footer", area.right(), true, |ui| {
                    if panel_toggle(ui, "panel-right", "detail", open) {
                        acts.push(Action::ToggleDetail);
                    }
                    // This build's version, at the other end.
                    if open {
                        app_buttons::footer_row(ui, Place::BottomRight);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!("gitcraft {}", crate::update::CURRENT))
                                    .size(11.0)
                                    .color(t.text_faint),
                            );
                            // Between the version and the buttons, the window can be moved.
                            let rest = ui.available_rect_before_wrap();
                            crate::app::title_bar::window_drag(ui, rest, "detail-footer");
                        });
                    }
                });
                if open {
                    self.detail_panel(ui, acts);
                } else {
                    let slid = widgets::slide_offset(ui, area.right(), true);
                    app_buttons::folded_column(ui, Place::BottomRight, slid);
                }
            },
        );
        if (show_branches, show_detail) != (settings.show_branches, settings.show_detail) {
            settings.show_branches = show_branches;
            settings.show_detail = show_detail;
            settings.set(&ctx);
        }
    }
}

/// F7 or Alt+↓ for the next change of a diff (`1`), Shift+F7 or Alt+↑ for the previous one.
pub(super) fn change_step(i: &egui::InputState) -> Option<i32> {
    let alt = i.modifiers.alt && !i.modifiers.command;
    if (i.key_pressed(Key::F7) && !i.modifiers.shift) || (alt && i.key_pressed(Key::ArrowDown)) {
        Some(1)
    } else if (i.key_pressed(Key::F7) && i.modifiers.shift) || (alt && i.key_pressed(Key::ArrowUp))
    {
        Some(-1)
    } else {
        None
    }
}

/// The footer button folding a side panel down to its strip, or opening it back up.
fn panel_toggle(ui: &mut Ui, icon: &str, name: &str, open: bool) -> bool {
    let tip = if open {
        format!("Hide the {name} panel")
    } else {
        format!("Show the {name} panel")
    };
    icons::tool_button(ui, icon, "", true)
        .named(&tip)
        .on_hover_text(tip)
        .clicked()
}
