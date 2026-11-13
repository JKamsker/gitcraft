//! A tiny fixture crate `fx` and its policy: `base`/`peer`/`other` are services (peers may
//! depend on each other), `alpha`/`beta` are features (they may not), `hub` coordinates.

#![allow(dead_code)]

use std::collections::BTreeMap;

use gitcraft_arch_fitness::analysis::Analysis;
use gitcraft_arch_fitness::policy::{Baseline, Policy};
use gitcraft_arch_fitness::ratchet::{self, Why};
use gitcraft_arch_fitness::{Outcome, analyze, check};

pub type Files = BTreeMap<String, String>;

pub fn files(entries: &[(&str, &str)]) -> Files {
    entries
        .iter()
        .map(|(p, s)| ((*p).to_owned(), (*s).to_owned()))
        .collect()
}

pub const LIB: &str = "mod base; mod peer; mod other; mod alpha; mod beta; pub mod hub;";

pub const HUB: &str = "
pub struct Tab { pub(crate) status: String, pub(crate) list: Vec<u8>, pub(crate) alpha: crate::alpha::State }
impl Tab { pub fn new() -> Self { Tab { status: String::new(), list: Vec::new(), alpha: crate::alpha::State } } }
";

/// The fixture crate with these extra/replaced files.
pub fn tree(extra: &[(&str, &str)]) -> Files {
    let mut f = files(&[
        ("src/lib.rs", LIB),
        ("src/base.rs", "pub fn util() {}"),
        ("src/peer.rs", "pub fn help() {}"),
        ("src/other.rs", "pub fn more() {}"),
        (
            "src/alpha.rs",
            "pub struct State; pub fn run() { crate::base::util(); }",
        ),
        (
            "src/beta.rs",
            "pub struct Thing; impl Thing { pub fn new() -> Self { Thing } } pub fn go() {}",
        ),
        ("src/hub.rs", HUB),
    ]);
    for (path, source) in extra {
        f.insert((*path).to_owned(), (*source).to_owned());
    }
    f
}

pub fn policy() -> Policy {
    let json = r#"{
      "schema": 1, "version": "test",
      "crates": [{"name": "fx", "root": "src/lib.rs", "cfg": {"recording": false}}],
      "layers": [
        {"name": "services", "peers": true, "description": ""},
        {"name": "features", "peers": false, "description": ""},
        {"name": "coordinator", "peers": false, "description": ""}
      ],
      "domains": {
        "base": {"layer": "services", "owner": "o", "description": "", "modules": ["fx::base"]},
        "peer": {"layer": "services", "owner": "o", "description": "", "modules": ["fx::peer"]},
        "other": {"layer": "services", "owner": "o", "description": "", "modules": ["fx::other"]},
        "alpha": {"layer": "features", "owner": "o", "description": "", "modules": ["fx::alpha"]},
        "beta": {"layer": "features", "owner": "o", "description": "", "modules": ["fx::beta"]},
        "hub": {"layer": "coordinator", "owner": "o", "description": "", "modules": ["=fx", "fx::hub"]}
      },
      "allowed_edges": [{"from": "layer:features", "to": "hub", "reason": "extension", "cycles": false}],
      "repo_tab": {"path": "fx::hub::Tab", "coordinators": ["hub"], "owners": {}, "owner_layers": ["features"], "default_owner": "hub"},
      "symbol_budget_domains": ["alpha"]
    }"#;
    match serde_json::from_str(json) {
        Ok(policy) => policy,
        Err(e) => unreachable!("fixture policy: {e}"),
    }
}

pub fn analysis(files: &Files) -> Analysis {
    analyze(files, &policy(), Default::default())
}

/// The baseline `adopt` records for these files: every current violation accepted.
pub fn adopted(files: &Files) -> Baseline {
    let why = Why {
        reason: "fixture".into(),
        owner: "o".into(),
        remediation: "none".into(),
    };
    ratchet::adopt(
        &analysis(files),
        &policy().symbol_budget_domains,
        &Baseline::default(),
        &why,
    )
}

pub fn outcome(files: &Files, baseline: &Baseline) -> Outcome {
    check(&analysis(files), &policy(), baseline)
}

pub fn rules(outcome: &Outcome) -> Vec<String> {
    let mut rules: Vec<String> = outcome.violations.iter().map(|v| v.rule.clone()).collect();
    rules.dedup();
    rules
}

pub fn has_edge(a: &Analysis, from: &str, to: &str) -> bool {
    a.edges.iter().any(|e| e.from == from && e.to == to)
}

pub fn edge_symbols(a: &Analysis, from: &str, to: &str) -> Vec<String> {
    a.edges
        .iter()
        .find(|e| e.from == from && e.to == to)
        .map(|e| e.symbols.keys().cloned().collect())
        .unwrap_or_default()
}
