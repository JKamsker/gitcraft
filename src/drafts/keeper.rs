//! When the drafts are saved: every few seconds, when the window loses the focus, at once when
//! one is used up (committed, sent, a rebase started or cancelled), and at exit.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::RepoDrafts;
use crate::repo::DraftShape;

/// How often what is typed is saved.
pub(super) const EVERY: Duration = Duration::from_secs(3);

/// Decides when to save; the window gives it what its tabs have, and it hands that on.
pub(crate) struct Keeper {
    saved_at: Instant,
    focused: bool,
    /// The drafts' shapes when last saved, by repository.
    shapes: HashMap<PathBuf, DraftShape>,
}

impl Default for Keeper {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl Keeper {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            saved_at: now,
            focused: true,
            shapes: HashMap::new(),
        }
    }

    /// Whether to save now, given whether the window has the focus and what shapes the tabs'
    /// drafts have.
    pub(crate) fn due(
        &mut self,
        now: Instant,
        focused: bool,
        shapes: &[(PathBuf, DraftShape)],
    ) -> bool {
        let lost_focus = self.focused && !focused;
        self.focused = focused;
        let used_up = shapes.iter().any(|(path, shape)| {
            self.shapes
                .get(path)
                .is_some_and(|before| shape.lost_since(*before))
        });
        lost_focus || used_up || now.duration_since(self.saved_at) >= EVERY
    }

    /// Saves `drafts` (by repository), written in the background; `shapes` are theirs.
    pub(crate) fn save(
        &mut self,
        now: Instant,
        drafts: Vec<(PathBuf, RepoDrafts)>,
        shapes: Vec<(PathBuf, DraftShape)>,
    ) {
        self.saved_at = now;
        for (path, drafts) in drafts {
            super::save(&path, drafts);
        }
        self.shapes.extend(shapes);
    }

    /// How long until the next save is due: a frame not saved wakes the window then, so that
    /// the last keys typed before it went idle are saved too.
    pub(crate) fn next_in(&self, now: Instant) -> Duration {
        EVERY.saturating_sub(now.duration_since(self.saved_at))
    }
}
