//! The fitness rules: compare an analysis with the policy and the reviewed baseline.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::analysis::Analysis;
use crate::domains::Edge;
use crate::policy::{Baseline, Policy};

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    pub rule: String,
    pub message: String,
    pub location: Option<String>,
    pub symbol: Option<String>,
    pub fix: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Outcome {
    pub violations: Vec<Violation>,
    /// Budgets above the measurement: lower them (`ratchet`) to lock in the improvement.
    pub ratchet: Vec<String>,
}

/// Gaps that make the inventory untrustworthy rather than merely approximate.
pub const BLOCKING_GAPS: &[&str] = &[
    "parse-error",
    "missing-file",
    "missing-module-file",
    "ambiguous-module-file",
    "unreached-file",
    "unknown-extern-crate",
];

const RATCHET: &str = "run `cargo run --manifest-path ci/arch-fitness/Cargo.toml -- ratchet` locally and commit the smaller baseline";

fn v(
    rule: &str,
    message: String,
    location: Option<String>,
    symbol: Option<String>,
    fix: &str,
) -> Violation {
    Violation {
        rule: rule.to_owned(),
        message,
        location,
        symbol,
        fix: fix.to_owned(),
    }
}

fn first_site(edge: &Edge) -> (Option<String>, Option<String>) {
    edge.samples.first().map_or((None, None), |s| {
        (
            Some(format!("{}:{}", s.file, s.line)),
            Some(format!("{} (written `{}`)", s.symbol, s.written)),
        )
    })
}

pub fn check(a: &Analysis, policy: &Policy, baseline: &Baseline) -> Outcome {
    let mut out = Outcome::default();
    for error in policy.validate() {
        out.violations.push(v(
            "policy",
            error,
            None,
            None,
            "fix ci/arch-fitness/policy.json",
        ));
    }
    gaps(a, &mut out);
    edges(a, policy, baseline, &mut out);
    cycles(a, baseline, &mut out);
    mutations(a, baseline, &mut out);
    reexports(a, baseline, &mut out);
    budgets(a, policy, baseline, &mut out);
    out.violations.sort();
    out
}

fn gaps(a: &Analysis, out: &mut Outcome) {
    for gap in a
        .gaps
        .items
        .iter()
        .filter(|g| BLOCKING_GAPS.contains(&g.kind.as_str()))
    {
        out.violations.push(v(
            "analysis-gap",
            format!("{}: {}", gap.kind, gap.detail),
            Some(format!("{}:{}", gap.file, gap.line)),
            None,
            "make the file part of a crate in policy.json `crates`, or list it in `ignored_files` with a reason",
        ));
    }
    if !a.repo_tab.found {
        out.violations.push(v(
            "analysis-gap",
            format!("the coordinator struct {} was not found", a.repo_tab.path),
            None,
            None,
            "update repo_tab.path in policy.json after moving the struct",
        ));
    }
    if let Some(domain) = a
        .domains
        .get(crate::domains::UNMAPPED)
        .filter(|d| d.modules > 0)
    {
        out.violations.push(v(
            "unmapped-module",
            format!(
                "{} modules belong to no domain: {}",
                domain.modules,
                a.unmapped.join(", ")
            ),
            None,
            None,
            "add the module prefix to a domain's `modules` in policy.json",
        ));
    }
}

fn edges(a: &Analysis, policy: &Policy, baseline: &Baseline, out: &mut Outcome) {
    let known: BTreeSet<(&str, &str)> = baseline
        .forbidden_edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    let mut seen = BTreeSet::new();
    for edge in a.edges.iter().filter(|e| e.policy == "forbidden") {
        let key = (edge.from.as_str(), edge.to.as_str());
        seen.insert(key);
        if known.contains(&key) {
            continue;
        }
        let layer = |d: &str| {
            policy
                .domains
                .get(d)
                .map_or("?", |s| s.layer.as_str())
                .to_owned()
        };
        let (location, symbol) = first_site(edge);
        out.violations.push(v(
            "forbidden-edge",
            format!(
                "new dependency {} ({}) -> {} ({}): {} references to {} symbols; this direction is not allowed",
                edge.from, layer(&edge.from), edge.to, layer(&edge.to), edge.references, edge.symbols.len()
            ),
            location,
            symbol,
            "depend on a lower layer instead (move the shared piece down, or pass a callback/event/Action up); if the boundary is wrong, add a reviewed `allowed_edges` entry with a reason to policy.json",
        ));
    }
    for stale in known.difference(&seen) {
        out.violations.push(v(
            "stale-baseline",
            format!("forbidden edge {} -> {} no longer exists", stale.0, stale.1),
            None,
            None,
            RATCHET,
        ));
    }
}

fn edge_set(a: &Analysis, members: &BTreeSet<&str>) -> BTreeSet<[String; 2]> {
    a.edges
        .iter()
        .filter(|e| members.contains(e.from.as_str()) && members.contains(e.to.as_str()))
        .map(|e| [e.from.clone(), e.to.clone()])
        .collect()
}

fn cycles(a: &Analysis, baseline: &Baseline, out: &mut Outcome) {
    let edges: BTreeMap<(&str, &str), &Edge> = a
        .edges
        .iter()
        .map(|e| ((e.from.as_str(), e.to.as_str()), e))
        .collect();
    for scc in &a.cycles {
        let members: BTreeSet<&str> = scc.iter().map(String::as_str).collect();
        let owner = baseline
            .cycles
            .iter()
            .find(|c| members.iter().all(|m| c.domains.iter().any(|d| d == m)));
        let Some(owner) = owner else {
            out.violations.push(v(
                "domain-cycle",
                format!("new dependency cycle between domains {}", scc.join(", ")),
                None,
                None,
                "break the cycle: remove the newest edge between these domains (see `edges` in the report), usually the upward one",
            ));
            continue;
        };
        let allowed: BTreeSet<&[String; 2]> = owner.edges.iter().collect();
        for pair in edge_set(a, &members) {
            if allowed.contains(&pair) {
                continue;
            }
            let (location, symbol) = edges
                .get(&(pair[0].as_str(), pair[1].as_str()))
                .map_or((None, None), |e| first_site(e));
            out.violations.push(v(
                "cycle-edge",
                format!("new edge {} -> {} inside the existing cycle {}", pair[0], pair[1], scc.join(", ")),
                location,
                symbol,
                "do not deepen a known cycle; route through the lower domain's API or an event instead",
            ));
        }
    }
    for entry in &baseline.cycles {
        let members: BTreeSet<&str> = a
            .cycles
            .iter()
            .filter(|scc| scc.iter().all(|m| entry.domains.contains(m)))
            .flatten()
            .map(String::as_str)
            .collect();
        let current: Vec<[String; 2]> = edge_set(a, &members).into_iter().collect();
        let domains: Vec<&str> = entry.domains.iter().map(String::as_str).collect();
        let gone = entry.edges.iter().any(|e| !current.contains(e));
        if members.into_iter().collect::<Vec<_>>() != domains || gone {
            out.violations.push(v(
                "stale-baseline",
                format!(
                    "cycle {} shrank (fewer domains or edges)",
                    entry.domains.join(", ")
                ),
                None,
                None,
                RATCHET,
            ));
        }
    }
}

fn mutations(a: &Analysis, baseline: &Baseline, out: &mut Outcome) {
    let known: BTreeSet<(&str, &str)> = baseline
        .repo_tab_mutations
        .iter()
        .map(|m| (m.domain.as_str(), m.field.as_str()))
        .collect();
    let mut seen = BTreeSet::new();
    for m in &a.repo_tab.cross_mutations {
        let key = (m.domain.as_str(), m.field.as_str());
        seen.insert(key);
        if known.contains(&key) {
            continue;
        }
        let location = m
            .samples
            .first()
            .map(|s| format!("{}:{} ({:?})", s.file, s.line, s.mode));
        out.violations.push(v(
            "repo-tab-mutation",
            format!("domain {} now mutates RepoTab.{} owned by {} ({} sites)", m.domain, m.field, m.owner, m.count),
            location,
            Some(format!("RepoTab.{}", m.field)),
            "call a method of the owning feature (or return an Action/effect for the coordinator) instead of writing the field; if ownership moved, update repo_tab.owners in policy.json",
        ));
    }
    for stale in known.difference(&seen) {
        out.violations.push(v(
            "stale-baseline",
            format!("{} no longer mutates RepoTab.{}", stale.0, stale.1),
            None,
            None,
            RATCHET,
        ));
    }
}

fn reexports(a: &Analysis, baseline: &Baseline, out: &mut Outcome) {
    let known: BTreeSet<(&str, &str, &str)> = baseline
        .reexports
        .iter()
        .map(|r| (r.module.as_str(), r.path.as_str(), r.target_domain.as_str()))
        .collect();
    let mut seen = BTreeSet::new();
    for r in &a.reexports {
        let key = (r.module.as_str(), r.path.as_str(), r.target_domain.as_str());
        seen.insert(key);
        if known.contains(&key) {
            continue;
        }
        out.violations.push(v(
            "visibility-reexport",
            format!("{} ({}) re-exports {} from domain {}", r.module, r.domain, r.target, r.target_domain),
            Some(format!("{}:{}", r.file, r.line)),
            Some(r.target.clone()),
            "import from the owning domain where it is used instead of widening this module's API, or make the `use` private",
        ));
    }
    for stale in known.difference(&seen) {
        out.violations.push(v(
            "stale-baseline",
            format!("re-export `{}` in {} is gone", stale.1, stale.0),
            None,
            None,
            RATCHET,
        ));
    }
}

/// Foreign symbols a domain references once: where a newly added dependency usually is.
fn single_uses(a: &Analysis, domain: &str) -> String {
    let mut uses: Vec<String> = a
        .edges
        .iter()
        .filter(|e| e.from == domain)
        .flat_map(|e| e.symbols.iter())
        .filter(|(_, u)| u.references == 1)
        .map(|(symbol, u)| format!("{symbol} ({}:{})", u.file, u.line))
        .collect();
    uses.sort();
    let more = uses.len().saturating_sub(10);
    uses.truncate(10);
    let suffix = if more > 0 {
        format!("; and {more} more")
    } else {
        String::new()
    };
    format!("referenced once (likely new): {}{suffix}", uses.join("; "))
}

fn budgets(a: &Analysis, policy: &Policy, baseline: &Baseline, out: &mut Outcome) {
    for (name, d) in &a.domains {
        if d.modules == 0 || name == crate::domains::UNMAPPED {
            continue;
        }
        let Some(budget) = baseline.budgets.get(name) else {
            out.violations.push(v(
                "fan-out-budget",
                format!(
                    "domain {name} has no budget (fan-out {}, exported {})",
                    d.fan_out, d.exported
                ),
                None,
                None,
                "add a reviewed budget for the new domain to baseline.json",
            ));
            continue;
        };
        let targets = || {
            let mut out: Vec<(usize, &str)> = a
                .edges
                .iter()
                .filter(|e| &e.from == name)
                .map(|e| (e.references, e.to.as_str()))
                .collect();
            out.sort();
            out.iter()
                .map(|(n, to)| format!("{to} ({n} refs)"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if d.fan_out > budget.domains {
            out.violations.push(v(
                "fan-out-budget",
                format!("domain {name} depends on {} domains, budget {}: {}", d.fan_out, budget.domains, targets()),
                None,
                None,
                "remove the new dependency; a budget increase needs a reviewed reason in the pull request",
            ));
        }
        if let (true, Some(limit)) = (policy.symbol_budget_domains.contains(name), budget.symbols)
            && d.fan_out_symbols > limit
        {
            out.violations.push(v(
                "fan-out-budget",
                format!("domain {name} references {} foreign symbols, budget {limit}", d.fan_out_symbols),
                None,
                Some(single_uses(a, name)),
                "use the existing APIs of the domains it already depends on, or move the logic to the owner",
            ));
        }
        if d.exported > budget.exported {
            let candidates = a
                .exported_unused
                .get(name)
                .map(|c| c.join("; "))
                .unwrap_or_default();
            out.violations.push(v(
                "visibility-budget",
                format!("domain {name} exposes {} items beyond its boundary, budget {}", d.exported, budget.exported),
                None,
                Some(candidates),
                "narrow new items to private, pub(super) or pub(in <domain root>); the listed items are exported but unused by other domains",
            ));
        }
        let lower = d.fan_out < budget.domains
            || d.exported < budget.exported
            || budget.symbols.is_some_and(|s| d.fan_out_symbols < s);
        if lower {
            out.ratchet.push(format!(
                "{name}: fan-out {}/{}, exported {}/{}",
                d.fan_out, budget.domains, d.exported, budget.exported
            ));
        }
    }
}
