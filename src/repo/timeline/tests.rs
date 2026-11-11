//! Tests of the timeline's model: the tree undo and redo walk, its cap, keeping it on disk,
//! taking in what was done before it was read, the steps of a jump, and reading the reflog.

use std::collections::BTreeMap;

use super::observe::{describe_reflog, fallback_label, parse_reflog};
use super::snapshot::{commits_of, describe, restore_steps};
use super::store::{self, Saved};
use super::{Entry, MAX_ENTRIES, Origin, RefState, Timeline, changes, tree};
use crate::repo::UndoEntry;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn state(branch: &str, branches: &[(&str, &str)]) -> RefState {
    let branches: BTreeMap<String, String> = branches
        .iter()
        .map(|(n, c)| ((*n).to_owned(), (*c).to_owned()))
        .collect();
    RefState {
        branch: branch.to_owned(),
        head: branches.get(branch).cloned().unwrap_or_default(),
        branches,
    }
}

fn entry(label: &str) -> Entry {
    Entry {
        id: 0,
        parent: None,
        time: 0,
        label: label.to_owned(),
        origin: Origin::Gitcraft,
        before: RefState::default(),
        after: RefState::default(),
        files: Vec::new(),
        files_total: 0,
        undo: None,
        redo_child: None,
        receipt: None,
    }
}

fn labels(t: &Timeline, ids: &[u64]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| t.get(*id).map(|e| e.label.clone()))
        .collect()
}
include!("tests/history_and_storage.rs");
include!("tests/changes_and_jump.rs");
include!("tests/navigation.rs");
