//! The health section's state and reading.

use super::Read;
use crate::git::doctor::health::{self, Health};
use crate::repo::RepoTab;

/// What the section shows.
#[derive(Default)]
pub(crate) struct HealthState {
    pub(super) found: Read<Health>,
}

impl RepoTab {
    /// Reads the repository's health in the background.
    pub(super) fn load_health(&mut self, read: u64, ctx: &egui::Context) {
        let path = self.path.clone();
        // Another machine is always Linux (WSL, an SSH server).
        let unix_there =
            !cfg!(any(windows, target_os = "macos")) || crate::remote::is_remote(&path);
        let Some(d) = &mut self.doctor else {
            return;
        };
        d.health.found = None;
        self.then(
            ctx,
            move || health::health(&path, unix_there),
            move |tab, found, _| {
                if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read) {
                    d.health.found = Some(found);
                }
            },
        );
    }

    /// What the tab knows of its own loading, for the section.
    pub(super) fn loading_facts(&self) -> Loading {
        Loading {
            times: self.load_times.iter().copied().collect(),
            loaded: self.history.len(),
            more: self.has_more,
        }
    }
}

/// How the tab's reads went.
pub(crate) struct Loading {
    /// How long the last reloads took, oldest first.
    pub(super) times: Vec<std::time::Duration>,
    /// Commits of history loaded, and whether there are more.
    pub(super) loaded: usize,
    pub(super) more: bool,
}
