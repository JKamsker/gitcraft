//! Revision-bound review evidence. Forge objects remain untouched: relocated anchors are
//! local navigation hints and never authorize posting or applying a suggestion.
mod anchors;
mod store;

pub(super) use anchors::{Relocations, relocate_threads};

mod continuous;
pub(super) use continuous::{ReviewAct, ReviewState};
mod freshness;
pub(super) use freshness::{Freshness, FreshnessAct};

pub(super) use freshness::show as freshness_ui;
mod storyboard;
pub(super) use storyboard::{StoryAct, Storyboard, show as storyboard_ui};
