//! Read-only filesystem inspection primitives, shared with recovery preflight.
mod registration;
pub(in crate::git) use registration::same_registered_path;
#[cfg(unix)]
pub(in crate::git) mod unix;
