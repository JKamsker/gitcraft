//! Observation-only plans: recovery retirement currently reclaims zero bytes.
use super::{Completeness, Inventory};
use std::path::PathBuf;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    BuildOutputs,
    RetireToRecovery,
}
#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub operation: Operation,
    pub roots: Vec<PathBuf>,
    pub relative_scope: String,
    pub logical_bytes: Option<u64>,
    pub reclaim_now: Option<u64>,
    pub potential_purge: Option<u64>,
    pub blockers: Vec<String>,
    pub fingerprint: String,
    pub observed: i64,
}

pub(crate) fn previews(found: &Inventory) -> [Preview; 2] {
    let mut blockers = found.blockers.clone();
    if !found.complete() {
        blockers.push("resources-incomplete".into());
    }
    if found.staged + found.unstaged + found.conflicted + found.untracked > 0 {
        blockers.push("resources-dirty".into());
    }
    if found.presence.is_some() {
        blockers.push("resources-active".into());
    }
    // Owned terminal observations cannot establish the absence of external users.
    blockers.push("resources-external-unknown".into());
    let size = found.complete().then(|| found.logical_bytes());
    let mut build = Preview {
        operation: Operation::BuildOutputs,
        roots: found.candidates.clone(),
        relative_scope: "target/ (recognized Cargo artifacts only)".into(),
        logical_bytes: (found.measurements[2].status == Completeness::Complete)
            .then_some(found.measurements[2].bytes),
        reclaim_now: None,
        potential_purge: None,
        blockers: blockers.clone(),
        fingerprint: found.fingerprint.clone(),
        observed: found.collected,
    };
    if build.roots.is_empty() {
        build.blockers.push("resources-no-build".into());
    }
    // Never aggregate build and whole-tree sizes: these are alternative operations.
    let retire = Preview {
        operation: Operation::RetireToRecovery,
        roots: vec![found.root.clone()],
        relative_scope: ". (move to recovery)".into(),
        logical_bytes: size,
        reclaim_now: Some(0),
        potential_purge: size,
        blockers,
        fingerprint: found.fingerprint.clone(),
        observed: found.collected,
    };
    [build, retire]
}
