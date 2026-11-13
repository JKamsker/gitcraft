//! The reviewed inputs: the domain policy (what may depend on what, who owns which state)
//! and the baseline (measured legacy exceptions and budgets, which only ratchet down).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: u32,
    /// Bumped on every reviewed change to the mapping or rules.
    pub version: String,
    pub crates: Vec<CrateSpec>,
    #[serde(default)]
    pub ignored_files: Vec<Ignored>,
    /// From lowest to highest: a domain may depend on domains of lower layers.
    pub layers: Vec<Layer>,
    pub domains: BTreeMap<String, DomainSpec>,
    #[serde(default)]
    pub allowed_edges: Vec<AllowedEdge>,
    pub repo_tab: RepoTabPolicy,
    /// Reserved: test-only code is always reported and never enforced, so `true` is rejected.
    #[serde(default)]
    pub enforce_tests: bool,
    /// Domains whose number of distinct foreign symbols is budgeted, not only their domains.
    #[serde(default)]
    pub symbol_budget_domains: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrateSpec {
    pub name: String,
    /// A root file; a `*` in its file name makes one crate per matching file.
    pub root: String,
    #[serde(default)]
    pub externs: BTreeMap<String, String>,
    #[serde(default)]
    pub test: bool,
    /// Configuration flags fixed for this crate (`{"recovery_recording": true}`).
    #[serde(default)]
    pub cfg: BTreeMap<String, bool>,
    /// The domain of modules no module pattern maps.
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ignored {
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    /// Whether domains of this layer may depend on each other.
    pub peers: bool,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainSpec {
    pub layer: String,
    pub owner: String,
    pub description: String,
    /// Module path prefixes (`gitcraft::repo::terminals`); the longest match wins.
    pub modules: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedEdge {
    /// A domain, or `layer:<name>` for every domain of a layer.
    pub from: String,
    pub to: String,
    pub reason: String,
    /// `false`: the edge is a reviewed composition pattern and not part of cycle analysis.
    #[serde(default = "yes")]
    pub cycles: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepoTabPolicy {
    /// `crate::module::Name` of the coordinator struct, as the analyzer names modules.
    pub path: String,
    /// Domains whose writes are orchestration: they may mutate any field.
    pub coordinators: Vec<String>,
    /// Explicit field owners. Other fields belong to the domain that defines their type when
    /// that domain is in an `owner_layers` layer, else to `default_owner`.
    pub owners: BTreeMap<String, Vec<String>>,
    pub owner_layers: Vec<String>,
    pub default_owner: String,
}

impl RepoTabPolicy {
    pub fn explicit_owner(&self, field: &str) -> Option<&str> {
        self.owners
            .iter()
            .find(|(_, fields)| fields.iter().any(|f| f == field))
            .map(|(domain, _)| domain.as_str())
    }
}

// Every exception records why it exists, who owns it and how it should go away.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EdgeException {
    pub from: String,
    pub to: String,
    pub reason: String,
    pub owner: String,
    pub remediation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CycleException {
    /// The members of one strongly connected component, sorted.
    pub domains: Vec<String>,
    /// Every domain edge inside it; a new one deepens the cycle.
    pub edges: Vec<[String; 2]>,
    pub reason: String,
    pub owner: String,
    pub remediation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MutationException {
    pub domain: String,
    pub field: String,
    pub reason: String,
    pub owner: String,
    pub remediation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReExportException {
    pub module: String,
    pub path: String,
    pub target_domain: String,
    pub reason: String,
    pub owner: String,
    pub remediation: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Distinct domains this domain depends on.
    pub domains: usize,
    /// Distinct foreign symbols it references (for `symbol_budget_domains`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbols: Option<usize>,
    /// Items visible beyond the domain (effective visibility).
    pub exported: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub schema: u32,
    pub note: String,
    pub forbidden_edges: Vec<EdgeException>,
    pub cycles: Vec<CycleException>,
    pub repo_tab_mutations: Vec<MutationException>,
    pub reexports: Vec<ReExportException>,
    pub budgets: BTreeMap<String, Budget>,
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn to_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

impl Policy {
    pub fn layer_rank(&self, domain: &str) -> Option<usize> {
        let layer = &self.domains.get(domain)?.layer;
        self.layers.iter().position(|l| &l.name == layer)
    }

    fn matches(&self, pattern: &str, domain: &str) -> bool {
        match pattern.strip_prefix("layer:") {
            Some(layer) => self.domains.get(domain).is_some_and(|d| d.layer == layer),
            None => pattern == domain,
        }
    }

    fn declared(&self, from: &str, to: &str) -> Option<&AllowedEdge> {
        self.allowed_edges
            .iter()
            .find(|e| self.matches(&e.from, from) && self.matches(&e.to, to))
    }

    /// Whether an edge takes part in cycle analysis.
    pub fn in_cycles(&self, from: &str, to: &str) -> bool {
        self.declared(from, to).is_none_or(|e| e.cycles)
    }

    /// Why an edge is permitted by the policy itself, or `None` when it is forbidden.
    pub fn allows(&self, from: &str, to: &str) -> Option<&'static str> {
        if self.declared(from, to).is_some() {
            return Some("declared");
        }
        let (Some(a), Some(b)) = (self.layer_rank(from), self.layer_rank(to)) else {
            return None;
        };
        if a > b {
            Some("layered")
        } else if a == b && self.layers[a].peers {
            Some("peer")
        } else {
            None
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (name, domain) in &self.domains {
            if !self.layers.iter().any(|l| l.name == domain.layer) {
                errors.push(format!("domain {name}: unknown layer {}", domain.layer));
            }
        }
        let known = |d: &str| self.domains.contains_key(d);
        let pattern = |p: &str| match p.strip_prefix("layer:") {
            Some(layer) => self.layers.iter().any(|l| l.name == layer),
            None => known(p),
        };
        for edge in &self.allowed_edges {
            if !pattern(&edge.from) || !pattern(&edge.to) {
                errors.push(format!(
                    "allowed edge {} -> {}: unknown domain",
                    edge.from, edge.to
                ));
            }
        }
        let tab = &self.repo_tab;
        for layer in &tab.owner_layers {
            if !self.layers.iter().any(|l| &l.name == layer) {
                errors.push(format!("repo_tab: unknown layer {layer}"));
            }
        }
        for domain in tab
            .coordinators
            .iter()
            .chain(tab.owners.keys())
            .chain([&tab.default_owner])
        {
            if !known(domain) {
                errors.push(format!("repo_tab: unknown domain {domain}"));
            }
        }
        for krate in &self.crates {
            if krate.domain.as_deref().is_some_and(|d| !known(d)) {
                errors.push(format!("crate {}: unknown domain", krate.name));
            }
        }
        if self.enforce_tests {
            errors.push("enforce_tests: holding test-only code to the rules is not implemented; it is reported only".into());
        }
        errors
    }
}
