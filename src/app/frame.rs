//! Frame composition and model polling for root and deferred windows.

use super::*;

impl App {
    /// The active tab's page under the strip.
    pub(super) fn active_tab_ui(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let active = self.active;
        let asking = self.askpass_open();
        match self.tabs.get_mut(active) {
            Some(Tab::Repo(repo)) => {
                repo.keys_blocked = self.palette.is_some()
                    || self.practice_open
                    || self.setup.open
                    || self.manual_open
                    || self.clone_form.is_some()
                    || self.remote_form.is_some()
                    || self.teleport.is_some()
                    || self.repo_scan.is_some()
                    || self.library_open
                    || self.my_work.open
                    || asking;
                // Its widgets' state (scroll offsets, text fields' undo, open menus) is its own:
                // every repository tab lays out the same widgets, which would otherwise share
                // it with the tab shown before.
                let scope = repo_scope(&repo.path);
                ui.push_id(scope, |ui| repo.ui(ui));
                // A tab asked for a folder to be opened.
                match repo.open_request.take() {
                    Some(OpenRequest::NewTab(path)) => self.open_repo(&path, &ctx),
                    Some(OpenRequest::Worktree(path)) => self.open_worktree(&path, &ctx),
                    None => {}
                }
            }
            Some(Tab::Settings) => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(t.dock))
                    .show(ui, |ui| self.settings_page(ui));
            }
            _ => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(t.dock))
                    .show(ui, |ui| self.home(ui));
            }
        }
    }

    /// Model work only: called by eframe even while the root has no painting pass, and
    /// by deferred children. Channel draining and service deadlines keep a single owner.
    pub(super) fn tick_model(&mut self, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
        // eframe also accepts Close from logic-only passes of hidden/minimized roots.
        self.close_root_with_children(ctx);
        self.wake_askpass_owner(ctx);
        self.settings = Settings::get(ctx);
        crate::portable::set_credential_store(self.settings.portable_credential_store);
        crate::network::set_choices(self.settings.offline, self.settings.pause_on_metered);
        crate::network::start_watching(ctx);
        self.take_handoffs(ctx);
        self.poll_resident(ctx);
        if self.git_version.is_err() {
            return;
        }
        self.poll_practice(ctx);
        self.poll_setup(ctx);
        self.poll_clone(ctx);
        self.poll_connecting(ctx);
        self.poll_launch(ctx);
        self.poll_library(ctx);
        self.poll_window_models(ctx);
        self.poll_update(ctx);
        self.poll_inbox_opening(ctx);
        self.agent_requests(ctx);
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
        {
            if let Tab::Repo(r) = tab {
                r.poll(ctx);
                r.background_tick();
                r.poll_parked(ctx);
            }
        }
        self.advance_patches(ctx);
        self.os_shell(ctx, frame);
        let focused = ctx.input(|i| i.raw.viewports.values().any(|v| v.focused == Some(true)));
        self.keep_drafts(ctx, focused);
        self.toast_finished(focused);
        // A dormant root still services completions/launches, even when every child is
        // minimized. This schedules logic; it does not force a hidden window to paint.
        if self.windows.enabled && !self.windows.parked.is_empty() {
            ctx.request_repaint_after_for(
                std::time::Duration::from_millis(250),
                egui::ViewportId::ROOT,
            );
        }
    }

    pub(super) fn window_ui(&mut self, ui: &mut Ui, frame: Option<&mut eframe::Frame>) {
        let ctx = ui.ctx().clone();
        if frame.is_none() {
            self.tick_model(&ctx, None);
        }
        if std::mem::take(&mut self.pick_on_start) {
            self.pick_existing(&ctx);
        }
        crate::image_cache::maintain(&ctx);
        let t = Tokens::get(&ctx);
        // A button dropped moves in the settings, read just below.
        crate::app_buttons::begin_frame(&ctx);
        // Column widths dragged in a graph header live in the context; keep them for saving.
        self.settings = Settings::get(&ctx);
        // A theme file being edited, VS Code's theme switched: shown as they change.
        theme::poll(&ctx);
        crate::portable::set_credential_store(self.settings.portable_credential_store);
        crate::network::set_choices(self.settings.offline, self.settings.pause_on_metered);
        crate::network::start_watching(&ctx);
        title_bar::apply(&ctx, &self.settings, &mut self.decorated);
        if let Some(frame) = frame {
            title_bar::sync_border(frame, &self.settings, &mut self.window_border);
        }
        self.close_root_with_children(&ctx);
        crate::app_buttons::set_current_theme(&ctx, &self.theme);

        // eframe shows the window after the first frame; maximize it on the next one.
        if MAXIMIZE_WHEN_SHOWN.load(Ordering::Relaxed) {
            if ctx.cumulative_frame_nr() >= 1 {
                MAXIMIZE_WHEN_SHOWN.store(false, Ordering::Relaxed);
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            } else {
                ctx.request_repaint();
            }
        }

        if let Err(err) = self.git_version.clone() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.dock))
                .show(ui, |ui| self.git_missing(ui, &err));
            return;
        }
        self.notice_profile_git(&ctx);
        let focused = self.shortcuts(&ctx);

        // The caption buttons go up to the window's corner.
        let strip_right = if title_bar::wanted(&self.settings) {
            0
        } else {
            6
        };
        egui::Panel::top("tabs")
            .frame(
                egui::Frame::new()
                    .fill(t.tab_strip)
                    .inner_margin(egui::Margin {
                        // The first tab starts at the window's edge: a gap before it shows as a
                        // dark band beside it when it is the active one.
                        left: 0,
                        right: strip_right,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| self.tab_strip(ui));
        self.update_banner(ui);
        if let Some(path) = self.switch_to.take() {
            self.open_worktree(&path, &ctx);
        }
        if std::mem::take(&mut self.close_worktree) {
            self.close_current_worktree(&ctx);
        }

        self.active_tab_ui(ui, &t);
        // Views popped out of any tab keep their windows, the tab shown or not; after it, so
        // they show what it just took in.
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let Tab::Repo(r) = tab {
                r.popouts_ui(&ctx, i == self.active);
            }
        }
        self.clone_dialog(&ctx);
        self.remote_dialog(&ctx);
        self.teleport_dialog(&ctx);
        self.askpass_dialog(&ctx);
        self.agent_card(&ctx);
        self.palette_ui(&ctx);
        self.manual_ui(&ctx);
        self.practice_dialog(&ctx);
        self.setup_ui(&ctx);
        crate::glossary::hover(&ctx);
        // After the tabs are drawn, so what was typed in this frame is in.
        self.keep_drafts(&ctx, focused);
        self.repo_scan_dialog(&ctx);
        self.library_dialog(&ctx);
        self.my_work_ui(&ctx);
        self.poll_diagnostics(&ctx);
        for (path, url) in crate::settings::network_page::take_rejections(&ctx) {
            for tab in &mut self.tabs {
                if let Tab::Repo(repo) = tab {
                    repo.forget_https_memory(&path, &url);
                }
            }
        }
        if crate::settings::network_page::take_inspection_request(&ctx) {
            crate::settings::show_page(&ctx, crate::settings::Page::HttpsSignins);
            self.open_settings();
        }
        self.toast_finished(focused);
        self.toasts_ui(&ctx);
        self.dropped_files(&ctx);
        self.app_button_events(&ctx);
        crate::app_buttons::end_frame(&ctx);
        if title_bar::wanted(&self.settings) {
            title_bar::resize_edges(&ctx);
        }
        keymap::which_key(&ctx, |command| match self.tabs.get(self.active) {
            Some(Tab::Repo(r)) => r.command_refusal(command),
            _ if command.scope() == keymap::Scope::Repository => {
                Some("Open a repository first".into())
            }
            _ => None,
        });
        crate::a11y::end_frame(&ctx);
    }
}
