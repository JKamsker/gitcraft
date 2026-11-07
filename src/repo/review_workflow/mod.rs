//! Revision-bound review evidence. Forge objects remain untouched: relocated anchors are
//! local navigation hints and never authorize posting or applying a suggestion.
mod anchors;
mod store;

pub(super) use anchors::{Relocations, relocate_threads};
