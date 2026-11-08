//! What later starts of gitcraft ask this window (see `crate::instance`): open a folder, or
//! just come to the front.

use crate::instance::Request;

use super::App;

impl App {
    /// Does what later starts asked since the last frame.
    pub(super) fn take_handoffs(&mut self, ctx: &egui::Context) {
        for request in crate::instance::take_requests() {
            self.handle_handoff(request, ctx);
        }
    }

    /// Does `request`: opens its folder as the command line does on start (a tab for it, or the
    /// one it has), and brings the window to the front.
    pub(super) fn handle_handoff(&mut self, request: Request, ctx: &egui::Context) {
        self.windows.focused_elsewhere = false;
        match request {
            Request::Focus => {}
            // Never queued here: an agent's call goes to the tab of its repository, with its
            // answer (see `take_agent_calls`).
            Request::Agent(_) => return,
            Request::Open(path) => {
                // Without git there's nothing to open it with: the window says so already.
                if self.git_version.is_ok() {
                    self.manual_open = false;
                    self.palette = None;
                    self.open_repo(&path, ctx);
                }
            }
            Request::Launch { cwd, args } => {
                let parsed = crate::launch::parse(args.iter().map(std::ffi::OsString::from));
                if let crate::launch::Parsed::Run(Some(cmd)) = parsed
                    && self.git_version.is_ok()
                {
                    self.manual_open = false;
                    self.palette = None;
                    self.launch(cmd, cwd, ctx);
                }
            }
        }
        if !self.windows.focused_elsewhere {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            come_to_front(ctx);
        }
    }
}

/// Brings the window to the front: restored if minimized, focused, and where the system keeps
/// another program's window in front (Windows, some Linux desktops), its taskbar button
/// flashes or its dock icon bounces until it is clicked.
fn come_to_front(ctx: &egui::Context) {
    // On macOS egui never learns that the window went to the Dock (only on start), and asking
    // to restore one that isn't minimized changes nothing there; elsewhere it would also undo
    // a maximized window, so it is asked only when known minimized.
    if cfg!(target_os = "macos") || ctx.input(|i| i.viewport().minimized == Some(true)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
        egui::UserAttentionType::Informational,
    ));
    ctx.request_repaint();
}
