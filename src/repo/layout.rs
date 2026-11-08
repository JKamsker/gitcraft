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
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.dock))
            .show(ui, |ui| {
                let pane_rect = ui.max_rect().intersect(ui.clip_rect());
                if self.file_comparison.is_some() {
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
            ui.add_space(18.0);
            ui.add(crate::spinner::Spinner::new().size(22.0));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(crate::i18n::text("repo-core-2879"))
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
            && !self.email.open
            && !self.devcontainer.open
            && !self.keys_blocked
            && !crate::a11y::keyboard_taken(ctx);
        // Ctrl+F, Esc and the steps of the find bar, when a diff, file or blame is shown.
        self.find_keys(ctx, free_keys);
        if free_keys {
            let commands = keymap::pressed(ctx, keymap::Scope::Repository);
            // While a commit's message is being edited, Ctrl+Enter saves it (the editor takes
            // the key also when nothing has the focus): it doesn't commit the changes too.
            let rewording = self.reword.is_some();
            acts.extend(
                commands
                    .into_iter()
                    .filter(|&c| !(rewording && c == keymap::Command::Commit))
                    .map(Action::Key),
            );
            let esc = ctx.input(|i| i.key_pressed(Key::Escape));
            let diff_keys = self.diff.is_some()
                && (self.main_keys(ctx, Panel::Diff) || self.conflict.is_some());
            if esc && self.file_comparison.is_some() {
                acts.push(Action::CloseFileComparison);
            } else if esc && self.close_drop_menu() {
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
            for command in keymap::pressed(ctx, keymap::Scope::Changes) {
                acts.push(Action::Key(command));
            }
        } else {
            keymap::retain_allowed(ctx);
        }
        if !free_keys && self.search.open && keymap::take(ctx, keymap::Command::Search) {
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
mod responsive_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn responsive_detail_collapses_and_reopens_without_losing_drafts_or_width()
    -> Result<(), Box<dyn std::error::Error>> {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/responsive-layout".into(), &ctx);
        tab.commit_summary = "keep this draft".into();
        tab.commit_body = "and its description".into();
        tab.amend = true;
        let hash = "a".repeat(40);
        tab.refs.head = hash.clone();
        tab.detail = Some(crate::git::CommitDetail {
            hash: hash.clone(),
            parents: Vec::new(),
            author: "Tester".into(),
            email: "test@example.invalid".into(),
            author_date: "2026-11-07".into(),
            committer: "Tester".into(),
            commit_date: "2026-11-07".into(),
            message: "Original subject".into(),
            files: Vec::new(),
            in_head: true,
            merges_after: false,
        });
        tab.reword = Some(crate::repo::Reword {
            hash,
            subject: "Editing survives collapse".into(),
            body: "Retain this edit too".into(),
            focus: true,
            fill: false,
        });
        let mut styled = false;
        let mut h = Harness::builder()
            .with_size([1100.0, 800.0])
            .build_ui_state(
                move |ui, tab: &mut RepoTab| {
                    if !styled {
                        crate::theme::install_fonts(ui.ctx());
                        crate::theme::apply(
                            ui.ctx(),
                            &crate::theme::ThemeId::default(),
                            &crate::theme::Tweaks::default(),
                        );
                        egui_extras::install_image_loaders(ui.ctx());
                        let mut settings = Settings::get(ui.ctx());
                        settings.show_branches = false;
                        settings.show_detail = true;
                        settings.set(ui.ctx());
                        styled = true;
                        return;
                    }
                    tab.side_panels(ui, &mut Vec::new());
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.label("Central content");
                    });
                },
                tab,
            );
        for _ in 0..5 {
            h.step();
        }
        assert!(h.query_by_label("Hide the detail panel").is_some());
        assert!(h.get_by_role(egui::accesskit::Role::TextInput).is_focused());
        let original = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
            .ok_or("responsive layout fixture lost its panel/editor state")?
            .size()
            .x;
        h.set_size(egui::vec2(360.0, 800.0));
        for _ in 0..6 {
            h.step();
        }
        assert!(h.query_by_label("Show the detail panel").is_some());
        assert!(
            Settings::get(&h.ctx).show_detail,
            "temporary collapse overwrote visibility preference"
        );
        assert_eq!(h.state().commit_summary, "keep this draft");
        assert_eq!(h.state().commit_body, "and its description");
        assert!(h.state().amend);
        crate::repo::detail::responsive_tests::evidence(&mut h, "detail-auto-collapsed");
        h.get_by_label("Show the detail panel").click();
        for _ in 0..6 {
            h.step();
        }
        assert!(
            h.query_by_label("Hide the detail panel").is_some(),
            "explicit reopening at the narrow width failed"
        );
        assert_eq!(
            h.state()
                .reword
                .as_ref()
                .ok_or("responsive layout fixture lost its panel/editor state")?
                .subject,
            "Editing survives collapse"
        );
        assert!(
            h.get_by_role(egui::accesskit::Role::TextInput).is_focused(),
            "reopening lost the editor's keyboard focus"
        );
        h.set_size(egui::vec2(1100.0, 800.0));
        for _ in 0..6 {
            h.step();
        }
        let restored = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
            .ok_or("responsive layout fixture lost its panel/editor state")?
            .size()
            .x;
        assert!(
            (original - restored).abs() < 1.0,
            "preferred width changed: {original} → {restored}"
        );
        assert!(h.query_by_label("Hide the detail panel").is_some());
        // A divider dragged below usable size collapses fully and retains a reopening width.
        let mut state = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
            .ok_or("responsive layout fixture lost its panel/editor state")?;
        state.outer_rect.set_width(180.0);
        h.ctx
            .data_mut(|d| d.insert_persisted(egui::Id::new("repo_detail"), state));
        for _ in 0..6 {
            h.step();
        }
        assert!(!Settings::get(&h.ctx).show_detail);
        assert!(h.query_by_label("Show the detail panel").is_some());
        assert_eq!(h.state().commit_summary, "keep this draft");
        Ok(())
    }
}

#[cfg(test)]
mod followup_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn harness(width: f32, height: f32, scale: f32) -> Harness<'static, (RepoTab, Vec<Action>)> {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/responsive-followup".into(), &ctx);
        tab.refs.head = "a".repeat(40);
        tab.refs.current = Some("main".into());
        tab.commits.push(std::sync::Arc::new(crate::git::Commit {
            hash: crate::repo::WIP_HASH.into(),
            ..Default::default()
        }));
        tab.selected = Some(0);
        tab.commit_summary = "keep this short-height draft".into();
        tab.commit_body = "and its description".into();
        tab.identity.name = "Test author".into();
        tab.identity.email = "author@example.invalid".into();
        tab.work.staged.push(crate::git::StatusFile {
            path: "src/file.rs".into(),
            old_path: None,
            area: crate::git::Area::Staged,
            code: 'M',
        });
        tab.focus_commit_summary = true;
        let mut styled = false;
        Harness::builder()
            .with_size([width / scale, height / scale])
            .with_pixels_per_point(scale)
            .build_ui_state(
                move |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| {
                    if !styled {
                        crate::theme::install_fonts(ui.ctx());
                        crate::theme::apply(
                            ui.ctx(),
                            &crate::theme::ThemeId::default(),
                            &Default::default(),
                        );
                        egui_extras::install_image_loaders(ui.ctx());
                        let mut settings = Settings::get(ui.ctx());
                        settings.show_branches = false;
                        settings.show_detail = true;
                        settings.set(ui.ctx());
                        styled = true;
                        return;
                    }
                    // Reserve room for app chrome and a wrapped repository toolbar, including the
                    // extra height that the parent's integrated toolbar uses in narrow windows.
                    egui::Panel::top("followup-toolbar")
                        .exact_size(70.0)
                        .show(ui, |ui| {
                            ui.label("Repository toolbar");
                        });
                    tab.side_panels(ui, acts);
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.label("History");
                    });
                },
                (tab, Vec::new()),
            )
    }

    #[test]
    fn responsive_followup_full_height_composer_needs_no_scrolling_after_resize()
    -> Result<(), Box<dyn std::error::Error>> {
        for scale in [1.0, 1.25, 1.5] {
            let mut h = harness(1400.0, 860.0, scale);
            for _ in 0..8 {
                h.step();
            }
            // A persisted small panel size must not cap the composition area forever.
            let mut panel = egui::PanelState::load(&h.ctx, egui::Id::new("commit_box"))
                .ok_or("composer panel")?;
            panel.outer_rect.set_height(90.0);
            h.ctx
                .data_mut(|d| d.insert_persisted(egui::Id::new("commit_box"), panel));
            for _ in 0..6 {
                h.step();
            }
            for resized in [false, true] {
                if resized {
                    h.set_size(egui::vec2(800.0 / scale, 400.0 / scale));
                    for _ in 0..6 {
                        h.step();
                    }
                    if h.query_by_label("Show the detail panel").is_some() {
                        h.get_by_label("Show the detail panel").click();
                        for _ in 0..6 {
                            h.step();
                        }
                    }
                    h.get_by_label("Commit 1 file").scroll_to_me();
                    for _ in 0..6 {
                        h.step();
                    }
                    h.set_size(egui::vec2(1400.0 / scale, 860.0 / scale));
                    for _ in 0..8 {
                        h.step();
                    }
                }
                let panel = egui::PanelState::load(&h.ctx, egui::Id::new("commit_box"))
                    .ok_or("composer panel")?
                    .outer_rect;
                let commit = h.get_by_label("Commit 1 file").rect();
                assert!(
                    panel.shrink(2.0).contains_rect(commit),
                    "commit clipped at {scale}, resized={resized}: {commit:?}, panel {panel:?}"
                );
                for field in h.query_all_by_role(egui::accesskit::Role::TextInput) {
                    assert!(
                        panel.shrink(2.0).contains_rect(field.rect()),
                        "message field clipped at {scale}, resized={resized}: {:?}, panel {panel:?}",
                        field.rect()
                    );
                }
                let toolbar = h.get_by_label("Message options").rect();
                assert!(
                    panel.shrink(2.0).contains_rect(toolbar),
                    "toolbar scrolled out at full height"
                );
                // Check real pointer reachability, not just an offscreen accessibility node.
                h.get_by_label("Commit 1 file").click();
                h.step();
                assert!(h.state().1.iter().any(|a| matches!(a, Action::Commit)));
                h.state_mut().1.clear();
                crate::repo::detail::responsive_tests::evidence(
                    &mut h,
                    &format!("followup-full-composer-1400-860-{scale}-{resized}"),
                );
            }
        }
        Ok(())
    }

    #[test]
    fn responsive_followup_short_reopen_scrolls_composer_actions_into_view() {
        for height in [400.0, 500.0, 650.0] {
            for scale in [1.0, 1.25, 1.5] {
                let mut h = harness(800.0, height, scale);
                for _ in 0..6 {
                    h.step();
                }
                if h.query_by_label("Show the detail panel").is_some() {
                    h.get_by_label("Show the detail panel").click();
                    for _ in 0..6 {
                        h.step();
                    }
                }
                assert!(h.query_by_label("Hide the detail panel").is_some());
                h.get_by_label("Commit 1 file").scroll_to_me();
                for _ in 0..6 {
                    h.step();
                }
                crate::repo::detail::responsive_tests::evidence(
                    &mut h,
                    &format!("followup-short-composer-{height}-{scale}"),
                );
                let r = h.get_by_label("Commit 1 file").rect();
                assert!(
                    r.top() >= 70.0 && r.bottom() < height / scale - 20.0,
                    "commit action outside detail viewport at {height}/{scale}: {r:?}"
                );
                h.get_by_label("Commit 1 file").click();
                h.step();
                assert!(
                    h.state().1.iter().any(|a| matches!(a, Action::Commit)),
                    "commit action could not be clicked at {height}/{scale}"
                );
                h.get_by_label("Skip hooks").scroll_to_me();
                for _ in 0..6 {
                    h.step();
                }
                let r = h.get_by_label("Skip hooks").rect();
                assert!(r.top() >= 70.0 && r.bottom() < height / scale - 20.0);
                h.get_by_label("Skip hooks").click();
                h.step();
                for _ in 0..3 {
                    h.step();
                }
                crate::repo::detail::responsive_tests::evidence(
                    &mut h,
                    &format!("followup-short-toolbar-{height}-{scale}"),
                );
                assert!(h.state().0.skip_hooks.commit);
                assert_eq!(h.state().0.commit_summary, "keep this short-height draft");
                assert_eq!(h.state().0.commit_body, "and its description");
            }
        }
    }

    #[test]
    fn responsive_followup_forced_reopen_divider_collapses_and_restores_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut h = harness(800.0, 650.0, 1.5);
        for _ in 0..6 {
            h.step();
        }
        let mut settings = Settings::get(&h.ctx);
        settings.show_branches = true;
        settings.set(&h.ctx);
        h.set_size(egui::vec2(1100.0, 700.0));
        for _ in 0..6 {
            h.step();
        }
        let original = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
            .ok_or("detail state")?
            .size()
            .x;
        h.state_mut().0.amend = true;
        h.state_mut().0.focus_commit_summary = true;
        h.step();
        h.set_size(egui::vec2(800.0 / 1.5, 650.0 / 1.5));
        for _ in 0..6 {
            h.step();
        }
        // This width fits until branches/chrome take room; shorten to force automatic collapse.
        h.set_size(egui::vec2(800.0 / 1.5, 500.0 / 1.5));
        for _ in 0..6 {
            h.step();
        }
        h.get_by_label("Show the detail panel").click();
        for _ in 0..6 {
            h.step();
        }
        h.set_size(egui::vec2(800.0 / 1.5, 650.0 / 1.5));
        for _ in 0..6 {
            h.step();
        }
        assert!(h.query_by_label("Hide the detail panel").is_some());
        h.state_mut().0.focus_commit_summary = true;
        h.step();
        assert!(
            h.query_all_by_role(egui::accesskit::Role::TextInput)
                .any(|n| n.is_focused())
        );
        // Drag the actual divider, matching the review's x≈270 -> 370 gesture.
        let rect = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
            .ok_or("expanded state")?
            .outer_rect;
        assert!(rect.width() >= DETAIL_COLLAPSE_W);
        let start = egui::pos2(rect.left(), rect.top() + 80.0);
        let end = egui::pos2(rect.right() - 163.0, start.y);
        h.event(egui::Event::PointerMoved(start));
        h.step();
        h.event(egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.event(egui::Event::PointerMoved(end));
        for _ in 0..3 {
            h.step();
        }
        h.event(egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        for _ in 0..6 {
            h.step();
        }
        crate::repo::detail::responsive_tests::evidence(
            &mut h,
            "followup-forced-divider-collapsed",
        );
        assert!(
            h.query_by_label("Show the detail panel").is_some(),
            "narrowing a forced-open panel left it expanded and blank"
        );
        assert!(!Settings::get(&h.ctx).show_detail);
        assert_eq!(h.state().0.commit_summary, "keep this short-height draft");
        assert!(h.state().0.amend);
        h.set_size(egui::vec2(1100.0, 700.0));
        for _ in 0..6 {
            h.step();
        }
        let mut settings = Settings::get(&h.ctx);
        settings.show_detail = true;
        settings.set(&h.ctx);
        for _ in 0..6 {
            h.step();
        }
        let restored = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
            .ok_or("restored state")?
            .size()
            .x;
        assert!(
            (original - restored).abs() < 1.0,
            "preferred width changed: {original} -> {restored}"
        );
        assert!(
            h.query_all_by_role(egui::accesskit::Role::TextInput)
                .any(|n| n.is_focused()),
            "editor focus lost across collapse/reopen"
        );
        Ok(())
    }
}
