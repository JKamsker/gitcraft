//! Linux: the launcher's progress (the `com.canonical.Unity.LauncherEntry` signal, which
//! GNOME's Dash to Dock / Ubuntu Dock, KDE's task manager and others show), and the recent
//! repositories, _Clone…_ and _Open…_ as actions of gitcraft's `.desktop` file.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use super::{MenuItem, TaskbarState};

/// The desktop file gitcraft's window belongs to, by its id.
fn desktop_id() -> &'static str {
    if crate::desktop_entry::appimage().is_some() {
        "gitcraft-appimage.desktop"
    } else {
        "gitcraft.desktop"
    }
}

/// The signal's body: the application, and what its launcher shows.
pub fn launcher_props(state: TaskbarState) -> (f64, bool, bool) {
    match state {
        TaskbarState::Idle => (0.0, false, false),
        // An unknown amount shows as a sliver, so that something shows.
        TaskbarState::Running(f) => (f64::from(f.unwrap_or(0.02)), true, false),
        TaskbarState::Failed => (1.0, false, true),
    }
}

/// Sends `state` to the launcher, from a thread of its own (the session bus is reached once).
pub fn set_progress(state: TaskbarState) {
    static SENDER: OnceLock<Option<std::sync::Mutex<Sender<TaskbarState>>>> = OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = channel::<TaskbarState>();
        std::thread::spawn(move || {
            let Ok(bus) = zbus::blocking::Connection::session() else {
                return;
            };
            let uri = format!("application://{}", desktop_id());
            while let Ok(state) = rx.recv() {
                let (progress, visible, urgent) = launcher_props(state);
                let mut props: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
                props.insert("progress", progress.into());
                props.insert("progress-visible", visible.into());
                props.insert("urgent", urgent.into());
                let _ = bus.emit_signal(
                    None::<&str>,
                    "/com/canonical/unity/launcherentry/gitcraft",
                    "com.canonical.Unity.LauncherEntry",
                    "Update",
                    &(uri.as_str(), props),
                );
            }
        });
        Some(std::sync::Mutex::new(tx))
    });
    if let Some(tx) = sender
        && let Ok(tx) = tx.lock()
    {
        let _ = tx.send(state);
    }
}

pub(super) fn set_menu(items: &[MenuItem]) {
    crate::desktop_entry::set_actions(items);
}
