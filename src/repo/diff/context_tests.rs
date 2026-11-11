//! Tests of the folds of unchanged lines: where they are, opening them, and that hunks and
//! lines still stage once they are open, on real repositories.

use std::path::{Path, PathBuf};

use super::*;
use crate::git::{self, DiffOptions};
use crate::repo::diff::{Pick, hunk_changes, load, parse, partial_patch};
use crate::repo::{DiffKey, DiffView};

// Fold geometry; included to preserve qualified test names.
include!("context_tests/fold_geometry.rs");

// Patch application; included to preserve qualified test names.
include!("context_tests/patch_application.rs");

// Restoration; included to preserve qualified test names.
include!("context_tests/restoration.rs");

// Empty ranges; included to preserve qualified test names.
include!("context_tests/empty_ranges.rs");
