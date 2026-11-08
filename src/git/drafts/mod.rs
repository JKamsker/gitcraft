//! Shared native publication core selected from owner 825d1f89; the full drafts
//! stack remains owned by batch C. Keep its full module when assembling upstream.
pub(in crate::git) mod index_transaction;
// Preserve the owner-frozen publisher algorithms verbatim. These two style
// lints belong to its source; the parent keeps batch C's integrated module.
#[allow(clippy::single_match, clippy::collapsible_if)]
mod native_files;
// Native rollback uses this bound; callers here retain the same 8 MiB limit.
pub const FILE_LIMIT: usize = 8 * 1024 * 1024;
