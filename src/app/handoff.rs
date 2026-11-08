//! What later starts of gitcraft ask this window (see `crate::instance`): open a folder, or
//! just come to the front.

use crate::instance::Request;

use super::App;

impl App {
    /// Does what later starts asked since the last frame.
    pub(super) fn take_handoffs(&mut self, ctx: &egui::Context) {
        for url in gitcraft_os::take_native_urls() {
            match url.and_then(native_url_request) {
                Ok(request) => self.handle_handoff(request, ctx),
                Err(why) => {
                    self.handle_handoff(Request::Focus, ctx);
                    self.report(why);
                }
            }
        }
        for args in gitcraft_os::take_native_dock_requests() {
            match args {
                Ok(args) => self.handle_handoff(
                    Request::Launch {
                        cwd: Default::default(),
                        args,
                    },
                    ctx,
                ),
                Err(why) => {
                    self.handle_handoff(Request::Focus, ctx);
                    self.report(why);
                }
            }
        }
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
        if self.windows.quit_requested {
            return;
        }
        if !self.windows.focused_elsewhere {
            self.root_shown();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            come_to_front(ctx);
        }
    }
}

/// Feed native delivery through the same parser/resolver/owner routing as single-instance
/// launch requests. This does not start another process or interpolate a shell command.
fn native_url_request(url: String) -> Result<Request, String> {
    match crate::launch::parse([std::ffi::OsString::from(&url)]) {
        crate::launch::Parsed::Run(Some(crate::launch::Command::Link(_))) => Ok(Request::Launch {
            cwd: std::path::PathBuf::new(),
            args: vec![url],
        }),
        crate::launch::Parsed::Fail(why) => Err(why),
        _ => Err("The native event did not contain a supported gitcraft link".into()),
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

#[cfg(test)]
mod native_url_tests {
    use super::*;
    #[test]
    fn native_url_adapter_preserves_pr_commit_file_fragments_and_parser_rejections()
    -> Result<(), String> {
        use crate::launch::{Command, Parsed, link::Target};
        for (url, expected) in [
            (
                "gitcraft://open?url=https://github.com/o/r/pull/12",
                Target::PullRequest(12),
            ),
            (
                "gitcraft://open?url=https://codeberg.org/o/r/commit/abc123",
                Target::Commit("abc123".into()),
            ),
            (
                "gitcraft://open?url=https://github.com/o/r/blob/main/file.rs%23L40",
                Target::File {
                    rev: "main".into(),
                    path: "file.rs".into(),
                    line: Some(40),
                },
            ),
        ] {
            let Request::Launch { cwd, args } = native_url_request(url.into())? else {
                return Err("native URL request must be a launch".into());
            };
            assert!(cwd.as_os_str().is_empty());
            let parsed = crate::launch::parse(args.iter().map(std::ffi::OsString::from));
            assert!(
                matches!(parsed, Parsed::Run(Some(Command::Link(link))) if link.target == expected)
            );
        }
        assert!(native_url_request("gitcraft://delete?url=x".into()).is_err());
        assert!(native_url_request("--clone".into()).is_err());
        Ok(())
    }
}
