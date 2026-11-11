//! gitcraft in the OS shell: the taskbar button's progress while a clone, fetch, pull or push
//! runs (red when it fails), and the menu of the taskbar button or launcher (a jump list on
//! Windows, the `.desktop` file's actions on Linux) with the recent repositories, _Clone…_ and
//! _Open…_.

#[cfg(target_os = "macos")]
mod macos;
mod menu;
#[cfg(target_os = "macos")]
pub use macos::{refresh as macos_refresh, set_progress as macos_progress};
mod taskbar;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

pub use menu::{CLONE_ARG, MenuItem, OPEN_ARG, StartAction, menu_items, start_action};
pub use taskbar::{Taskbar, TaskbarState};

#[cfg(target_os = "linux")]
pub use linux::set_progress as linux_progress;
#[cfg(windows)]
pub use windows::set_progress as windows_progress;

/// Puts `items` in the taskbar button's or launcher's menu, in the background (it writes a
/// file, or talks to the shell), on one thread that applies only the latest list asked for, so
/// that two quick changes can't land in the wrong order. A list like the last one asked for is
/// not asked for again.
pub fn set_menu(items: Vec<MenuItem>) {
    #[cfg(target_os = "macos")]
    {
        macos::set_menu(&items);
    }
    #[cfg(not(target_os = "macos"))]
    set_menu_in_worker(items);
}

#[cfg(not(target_os = "macos"))]
fn set_menu_in_worker(items: Vec<MenuItem>) {
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Mutex, OnceLock};
    static LAST: Mutex<Vec<MenuItem>> = Mutex::new(Vec::new());
    static WORKER: OnceLock<Mutex<Sender<Vec<MenuItem>>>> = OnceLock::new();
    if let Ok(mut last) = LAST.lock() {
        if *last == items {
            return;
        }
        last.clone_from(&items);
    }
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<Vec<MenuItem>>();
        std::thread::spawn(move || {
            while let Ok(mut items) = rx.recv() {
                // Only the latest of those waiting.
                while let Ok(newer) = rx.try_recv() {
                    items = newer;
                }
                #[cfg(windows)]
                windows::set_menu(&items);
                #[cfg(target_os = "linux")]
                linux::set_menu(&items);
                #[cfg(not(any(windows, target_os = "linux")))]
                let _ = items;
            }
        });
        Mutex::new(tx)
    });
    if let Ok(tx) = worker.lock() {
        let _ = tx.send(items);
    }
}
