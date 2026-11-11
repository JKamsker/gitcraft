//! The tab's layout: toolbar on top, the foldable side panels, the terminal and the central
//! view; and its keyboard shortcuts.

#[path = "core_seams/repository_shortcuts.rs"]
mod repository_shortcuts;

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
        let mut acts: Vec<Action> = self.take_queued_actions();
        self.dco_frame(&ctx);
        self.owners_frame(&ctx);
        self.forks_frame(&ctx);
        self.hotfix_frame(&ctx);
        self.keyboard_panes(&ctx);
        self.echo_frame(&ctx);
        self.intent_frame(&ctx);
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
        self.ssh_setup_window(&ctx);
        self.slow_banner(ui);
        self.practice_banner(ui);
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
        if self.terminals.is_open() && self.docked(Panel::Terminal) {
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
                let pane_rect = ui.max_rect().intersect(ui.clip_rect());
                if self.resolver.reviewing() {
                    self.resolver_review_view(ui, &mut acts);
                } else if self.file_comparison.is_some() {
                    self.file_comparison_view(ui, &mut acts);
                } else if self.conflict.is_some() {
                    self.conflict_view(ui, &mut acts);
                } else if self.rebase.is_some() {
                    self.rebase_view(ui, &mut acts);
                } else if self.symbols.open {
                    self.symbol_biography_view(ui, &mut acts);
                } else if self.dossier.open {
                    self.line_dossier_view(ui, &mut acts);
                } else if self.review_document.open {
                    self.review_document_ui(ui, &mut acts);
                } else if self.split.is_some() {
                    self.split_view(ui, &mut acts);
                } else if self.absorb.is_some() {
                    self.absorb_view(ui, &mut acts);
                } else if self.diff_beside_graph(&ctx) {
                    self.graph_diff_view(ui, &mut acts);
                } else if self.diff.is_some()
                    && !self.history_beside_diff()
                    && self.docked(Panel::Diff)
                {
                    self.diff_view(ui, &mut acts);
                } else if self.tour.is_some() {
                    self.tour_view(ui, &mut acts);
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
                } else if self.insights.is_some() {
                    self.insights_ui(ui, &mut acts);
                } else if self.doctor.is_some() {
                    self.doctor_ui(ui, &mut acts);
                } else {
                    if self.reflog.is_some() {
                        self.reflog_panel(ui, &mut acts);
                    }
                    self.graph_view(ui, &mut acts);
                }
                self.keyboard_pane(ui, super::keyboard::Pane::Graph, pane_rect);
            });

        self.resolver_card(&ctx, big_area.response.rect, &mut acts);
        self.drag_overlay(&ctx, &mut acts);
        self.dialogs(&ctx, &mut acts);
        self.keep_both_dialog(&ctx);
        self.ai_review_window(&ctx);
        self.review_tools_ui(&ctx, &mut acts);
        self.lfs_locks_ui(&ctx, &mut acts);
        self.ghost_frame(&ctx);
        self.collision_window(&ctx, &mut acts);
        self.lfs_storage_ui(&ctx, &mut acts);
        self.issue_picker(&ctx, &mut acts);
        self.email_window(&ctx, &mut acts);
        self.devcontainer_window(&ctx, &mut acts);
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
    /// repository read in an instant doesn't flash it; no spinner until the read is slow
    /// (see `slow_banner`).
    fn opening_screen(&self, ui: &mut Ui) {
        const QUIET: std::time::Duration = std::time::Duration::from_millis(150);
        let waited = self.opened.elapsed();
        if waited < QUIET {
            ui.ctx().request_repaint_after(QUIET - waited);
            return;
        }
        let slow = waited >= super::slow_banner::SLOW;
        if !slow {
            ui.ctx()
                .request_repaint_after(super::slow_banner::SLOW - waited);
        }
        let t = Tokens::get(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 70.0).max(0.0));
            icons::inline(ui, "folder-git-2", 40.0, t.text_faint);
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(crate::i18n::message(
                    "repo-core-2878",
                    &[("name", self.name.to_string().into())],
                ))
                .font(crate::theme::semibold(18.0))
                .color(t.text),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(self.path.display().to_string())
                    .size(12.0)
                    .color(t.text_faint),
            );
            if slow {
                ui.add_space(18.0);
                ui.add(crate::spinner::Spinner::new().size(22.0));
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(crate::i18n::message(
                        "repo-core-slow-load",
                        &[("name", self.name.to_string().into())],
                    ))
                    .size(12.5)
                    .color(t.warning),
                );
                ui.label(
                    egui::RichText::new(crate::i18n::text("repo-core-2879"))
                        .size(12.5)
                        .color(t.text_dim),
                );
            }
        });
    }

    /// The branches panel on the left and the detail panel on the right.
    pub(in crate::repo) fn side_panels(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
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
                    let pane_rect = ui.max_rect().intersect(ui.clip_rect());
                    self.sidebar(ui, acts);
                    self.keyboard_pane(ui, super::keyboard::Pane::Sidebar, pane_rect);
                } else {
                    let slid = widgets::slide_offset(ui, area.left(), false);
                    let below = app_buttons::folded_column(ui, Place::BottomLeft, slid);
                    self.folded_sidebar(ui, area.left(), below, acts);
                }
            },
        );
        // Keep a usable expanded width independently of temporary window compression.
        // A small panel folds completely; no minimum forces it over the central view.
        let detail_id = egui::Id::new("repo_detail");
        let remembered_id = detail_id.with("usable-width");
        let was_automatic_id = detail_id.with("auto-collapsed");
        let forced_id = detail_id.with("reopen-small");
        let was_automatic = ctx
            .data(|d| d.get_temp::<bool>(was_automatic_id))
            .unwrap_or(false);
        let was_forced = ctx.data(|d| d.get_temp::<bool>(forced_id)).unwrap_or(false);
        let previous_panel = egui::PanelState::load(&ctx, detail_id);
        let width = previous_panel.as_ref().map(|s| s.size().x).unwrap_or(420.0);
        let remembered = ctx
            .data(|d| d.get_temp::<f32>(remembered_id))
            .unwrap_or(420.0);
        if show_detail && width < DETAIL_COLLAPSE_W && !was_automatic {
            show_detail = false;
            if let Some(mut state) = egui::PanelState::load(&ctx, detail_id) {
                state.outer_rect.set_width(remembered);
                ctx.data_mut(|d| d.insert_persisted(detail_id, state));
            }
        } else if width >= DETAIL_COLLAPSE_W && !was_automatic && !was_forced {
            ctx.data_mut(|d| d.insert_temp(remembered_id, width));
        }
        let room = ui.available_width();
        let fits = detail_fits(room, ui.available_height());
        let mut forced = was_forced;
        if fits || !show_detail {
            forced = false;
        }
        let automatic = show_detail && !forced && !fits;
        let mut expanded = show_detail && !automatic;
        ctx.data_mut(|d| d.insert_temp(was_automatic_id, automatic));
        // Temporary compression, including an explicit reopen in a narrow window, never
        // becomes the preferred width. Restore before egui lays out the expanded panel.
        if ((was_automatic && !automatic) || (was_forced && fits))
            && let Some(mut state) = egui::PanelState::load(&ctx, detail_id)
        {
            state.outer_rect.set_width(remembered);
            ctx.data_mut(|d| d.insert_persisted(detail_id, state));
        }
        let visible_id = detail_id.with("was-visible");
        let focus_id = detail_id.with("saved-focus");
        let was_visible = ctx
            .data(|d| d.get_temp::<bool>(visible_id))
            .unwrap_or(false);
        if was_visible
            && let Some(focus) = ctx.memory(|m| m.focused())
            && let Some(response) = ctx.read_response(focus)
            && let Some(panel) = &previous_panel
        {
            // Remember interior focus before a divider/footer click takes it away. Those
            // edge controls must not replace the editor to restore after reopening.
            if panel.outer_rect.shrink(8.0).contains_rect(response.rect) {
                ctx.data_mut(|d| d.insert_temp(focus_id, focus));
            } else if !panel.outer_rect.intersects(response.rect) {
                // Focus deliberately moved to the graph or another panel.
                ctx.data_mut(|d| d.remove::<egui::Id>(focus_id));
            }
        }
        if !was_visible
            && expanded
            && let Some(focus) = ctx.data(|d| d.get_temp::<egui::Id>(focus_id))
        {
            ctx.data_mut(|d| d.remove::<egui::Id>(focus_id));
            ctx.memory_mut(|m| m.request_focus(focus));
        }
        ctx.data_mut(|d| d.insert_temp(visible_id, expanded));
        egui::Panel::show_switched(
            ui,
            &mut expanded,
            egui::Panel::right("repo_detail_folded")
                .resizable(true)
                .exact_size(widgets::FOLDED_W)
                .frame(side),
            egui::Panel::right("repo_detail")
                .resizable(true)
                .default_size(420.0)
                .size_range(0.0..=900.0)
                .frame(side),
            |ui, open| {
                widgets::panel_footer(ui, "repo_detail_footer", area.right(), true, |ui| {
                    if panel_toggle(ui, "panel-right", "detail", open && !automatic) {
                        if automatic {
                            // An explicit reopen gives the details priority until the window
                            // has room again. The repository and editors were never reset.
                            forced = true;
                        } else {
                            acts.push(Action::ToggleDetail);
                        }
                    }
                    // This build's version, at the other end.
                    if open && !automatic && ui.available_width() >= DETAIL_COLLAPSE_W {
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
                if open && !automatic && ui.available_width() >= DETAIL_COLLAPSE_W {
                    let pane_rect = ui.max_rect().intersect(ui.clip_rect());
                    self.detail_panel(ui, acts);
                    self.keyboard_pane(ui, super::keyboard::Pane::Detail, pane_rect);
                } else {
                    let slid = widgets::slide_offset(ui, area.right(), true);
                    app_buttons::folded_column(ui, Place::BottomRight, slid);
                }
            },
        );
        ctx.data_mut(|d| d.insert_temp(forced_id, forced));
        if !automatic {
            show_detail = expanded;
        }
        if (show_branches, show_detail) != (settings.show_branches, settings.show_detail) {
            settings.show_branches = show_branches;
            settings.show_detail = show_detail;
            settings.set(&ctx);
        }
    }
}

/// The footer button folding a side panel down to its strip, or opening it back up.
fn panel_toggle(ui: &mut Ui, icon: &str, name: &str, open: bool) -> bool {
    let tip = if open {
        crate::i18n::message("repo-core-2891", &[("name", name.to_string().into())])
    } else {
        crate::i18n::message("repo-core-2892", &[("name", name.to_string().into())])
    };
    icons::tool_button(ui, icon, "", true)
        .named(&tip)
        .on_hover_text(tip)
        .clicked()
}

/// Below this usable width, show the reopening strip instead of compressed controls.
const DETAIL_COLLAPSE_W: f32 = 240.0;

fn detail_fits(available_width: f32, available_height: f32) -> bool {
    available_width >= DETAIL_COLLAPSE_W + 200.0 && available_height >= 280.0
}

#[cfg(test)]
#[path = "direct_tests/layout_responsive_tests.rs"]
mod responsive_tests;

#[cfg(test)]
#[path = "direct_tests/layout_followup_tests.rs"]
mod followup_tests;
