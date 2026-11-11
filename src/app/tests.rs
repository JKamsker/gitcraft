//! Saved state, the active tab on start and the window icon.

use eframe::Storage as _;

use super::{Saved, active_among_kept, window_icon};
use crate::tab_look::{TabColor, TabLook};

/// A storage that keeps strings in memory, as eframe's own does on disk.
#[derive(Default)]
struct Memory(std::collections::HashMap<String, String>);

impl eframe::Storage for Memory {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

include!("tests_cases/persistence.rs");

include!("tests_cases/handoff.rs");

include!("tests_cases/launch_commands.rs");
