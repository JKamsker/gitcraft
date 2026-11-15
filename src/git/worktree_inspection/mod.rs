//! Read-only filesystem inspection primitives, shared with recovery preflight.
#[cfg(unix)]
pub(in crate::git) mod unix;
