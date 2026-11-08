//! Resident commands re-enter the same launch/window lifecycle as visible controls.
use super::App;
use crate::tray::Action;
impl App {
    pub(super) fn quit(&mut self, ctx: &egui::Context) {
        self.windows.quit_requested = true;
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
    }
    pub(super) fn poll_resident(&mut self, ctx: &egui::Context) {
        // Native commands belong to the coordinator, even if a detached viewport paints first.
        if self.windows.current != egui::ViewportId::ROOT {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            return;
        }
        if self.settings.keep_running_in_tray {
            ctx.request_repaint_after_for(
                std::time::Duration::from_millis(250),
                egui::ViewportId::ROOT,
            );
        }
        let mut open = Vec::new();
        let mut busy = std::collections::HashSet::new();
        for tab in self
            .tabs
            .iter()
            .chain(self.windows.parked.values().flat_map(|w| &w.tabs))
        {
            if let super::Tab::Repo(repo) = tab {
                open.push(repo.path.clone());
                if repo.is_busy() {
                    busy.insert(repo.path.clone());
                }
            }
        }
        self.resident.watcher.tick(
            crate::tray::watch::Inputs {
                enabled: self.settings.keep_running_in_tray && !self.windows.quit_requested,
                profile: self.settings.active_profile,
                open: &open,
                busy: &busy,
                recent: &self.recent,
                minutes: self.settings.auto_fetch_minutes,
                notifications_enabled: self.settings.notifications,
            },
            ctx,
        );
        let text = crate::network::background_paused().map_or_else(
            || self.resident.watcher.status.clone(),
            |pause| format!("{} — background checks paused", pause.label()),
        );
        for action in self.resident.sync(
            self.settings.keep_running_in_tray && !self.windows.quit_requested,
            &self.recent,
            &text,
            ctx,
        ) {
            match action {
                Action::Show => self.handle_handoff(crate::instance::Request::Focus, ctx),
                Action::Launch(args) => self.handle_handoff(
                    crate::instance::Request::Launch {
                        cwd: Default::default(),
                        args,
                    },
                    ctx,
                ),
                Action::Quit => self.quit(ctx),
            }
        }
    }
}

#[cfg(not(test))]
pub(crate) fn notification_body(text: &str) -> String {
    super::panels::notification_body(text)
}
