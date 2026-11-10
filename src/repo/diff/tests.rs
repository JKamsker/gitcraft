//! Tests for diff parsing, line pairing, partial patches, images, selection and the change map.

use std::collections::BTreeSet;

use egui::Color32;

use super::images::image_id;
use super::jump::{change_at, changes, offset_of, step};
use super::map::change_runs;
use super::paint::line_job;
use super::parse::{changed_ranges, hunk_start, is_new_file, meta};
use super::select::Side;
use super::*;
use crate::theme;

// Navigation selection; included to preserve qualified test names.
include!("tests/navigation_selection.rs");

// Patches; included to preserve qualified test names.
include!("tests/patches.rs");

// Presentation; included to preserve qualified test names.
include!("tests/presentation.rs");
