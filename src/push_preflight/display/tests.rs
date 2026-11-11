//! Inert supplied-state checks. No Git, processes, filesystem, native UI or discovery.
use super::*;
use crate::i18n::set_language;
use std::path::Path;

// Preserve qualified names used by platform test shards.
include!("tests/push.rs");
include!("tests/families.rs");
include!("tests/catalog.rs");
