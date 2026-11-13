//! Architecture fitness checks for GitCraft (issue #209).
//!
//! A `syn`-based inventory of every analyzed crate's module tree, resolved references and
//! `RepoTab` field accesses, aggregated into reviewed feature domains, and checked against
//! a boundary policy and a ratchet-only baseline. See docs/architecture-boundaries.md.

pub mod analysis;
pub mod collect;
mod declare;
pub mod domains;
mod macros;
pub mod markdown;
pub mod model;
mod places;
pub mod policy;
pub mod ratchet;
pub mod refs;
pub mod resolve;
pub mod rules;
pub mod source;
pub mod syntax;
pub mod tab;
pub mod tab_report;
mod visit;
mod walk;

use std::collections::BTreeMap;

use serde::Serialize;

pub use analysis::{Analysis, SourceInfo, analyze};
pub use policy::{Baseline, Policy};
pub use rules::{Outcome, Violation, check};

/// The JSON report: the inventory, and the check result when a baseline was given.
#[derive(Serialize)]
pub struct Report<'a> {
    #[serde(flatten)]
    pub analysis: &'a Analysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<&'a Outcome>,
}

/// Analyze sources and check them in one step (used by tests and the CLI).
pub fn run(
    sources: &BTreeMap<String, String>,
    policy: &Policy,
    baseline: &Baseline,
) -> (Analysis, Outcome) {
    let analysis = analyze(sources, policy, SourceInfo::default());
    let outcome = check(&analysis, policy, baseline);
    (analysis, outcome)
}
