//! gitcraft in the OS shell: the taskbar button's progress while a clone, fetch, pull or push
//! runs (red when it fails), and the menu of the taskbar button or launcher (a jump list on
//! Windows, the `.desktop` file's actions on Linux) with the recent repositories, _Clone…_ and
//! _Open…_.

mod menu;
mod taskbar;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

pub use menu::{MenuItem, StartAction, menu_items, start_action};
pub use taskbar::{Taskbar, TaskbarState};

#[cfg(target_os = "linux")]
pub use linux::set_progress as linux_progress;
#[cfg(windows)]
pub use windows::set_progress as windows_progress;

/// Puts `items` in the taskbar button's or launcher's menu, in the background (it writes a
/// file, or talks to the shell); a list like the last one put there is not put again.
pub fn set_menu(items: Vec<MenuItem>) {
    use std::sync::Mutex;
    static LAST: Mutex<Vec<MenuItem>> = Mutex::new(Vec::new());
    if let Ok(mut last) = LAST.lock() {
        if *last == items {
            return;
        }
        last.clone_from(&items);
    }
    std::thread::spawn(move || {
        #[cfg(windows)]
        windows::set_menu(&items);
        #[cfg(target_os = "linux")]
        linux::set_menu(&items);
        #[cfg(not(any(windows, target_os = "linux")))]
        let _ = items;
    });
}
