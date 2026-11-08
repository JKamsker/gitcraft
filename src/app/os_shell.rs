//! The window in the OS shell, each frame: the taskbar button's progress (the clone and the
//! tabs' network operations), the launcher's menu of recent repositories, and a folder to pick
//! when started to (`--open`).

use super::{App, Tab};
use crate::os_shell::{self, TaskbarState};

impl App {
    pub(super) fn os_shell(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.menu_for.as_ref() != Some(&self.recent) {
            self.menu_for = Some(self.recent.clone());
            os_shell::set_menu(os_shell::menu_items(&self.recent));
        }

        let mut running: Vec<Option<f32>> = Vec::new();
        let mut failed = std::mem::take(&mut self.clone_failed);
        if let Some(cloning) = &self.cloning {
            running.push(cloning.control.progress().and_then(|p| p.fraction()));
        }
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
        {
            if let Tab::Repo(repo) = tab {
                running.extend(repo.taskbar_progress());
                failed |= repo.take_network_failure();
            }
        }
        let focused = ctx.input(|i| i.viewport().focused) != Some(false);
        let state = self.taskbar.state(&running, failed, focused);
        if self.taskbar.should_send(state) && send(state, frame) {
            self.taskbar.sent(state);
        }
        // Throttled progress that moved is sent on a later frame, and red is sent again.
        if state != TaskbarState::Idle {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }
}

#[cfg(windows)]
fn send(state: TaskbarState, frame: &eframe::Frame) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = frame.window_handle() else {
        return false;
    };
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => crate::os_shell::windows_progress(h.hwnd.get(), state),
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn send(state: TaskbarState, _frame: &eframe::Frame) -> bool {
    crate::os_shell::linux_progress(state);
    true
}

#[cfg(not(any(windows, target_os = "linux")))]
fn send(_state: TaskbarState, _frame: &eframe::Frame) -> bool {
    true
}
