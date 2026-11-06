//! Windows: the jump list and the taskbar button's progress, through `gitcraft-os`.

use super::{MenuItem, TaskbarState};

fn link(item: &MenuItem) -> gitcraft_os::Link {
    gitcraft_os::Link {
        title: item.title.clone(),
        args: item.args.clone(),
        tooltip: item.tooltip.clone(),
    }
}

pub(super) fn set_menu(items: &[MenuItem]) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let recent: Vec<_> = items.iter().filter(|i| i.recent).map(link).collect();
    let tasks: Vec<_> = items.iter().filter(|i| !i.recent).map(link).collect();
    let _ = gitcraft_os::set_jump_list(&exe, "Recent repositories", &recent, &tasks);
}

/// Shows `state` on the taskbar button of the window `hwnd`.
pub fn set_progress(hwnd: isize, state: TaskbarState) {
    let progress = match state {
        TaskbarState::Idle => gitcraft_os::Progress::None,
        TaskbarState::Running(None) => gitcraft_os::Progress::Indeterminate,
        TaskbarState::Running(Some(f)) => gitcraft_os::Progress::Value(f),
        TaskbarState::Failed => gitcraft_os::Progress::Error,
    };
    let _ = gitcraft_os::set_taskbar_progress(hwnd, progress);
}
