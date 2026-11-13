//! Local-only baseline maintenance. `ratchet` only removes stale exceptions and lowers
//! budgets; `adopt` records current violations as exceptions and must be reviewed. Neither
//! runs in CI, so a baseline can never reset itself there.

use std::collections::BTreeSet;

use crate::analysis::Analysis;
use crate::policy::{
    Baseline, Budget, CycleException, EdgeException, MutationException, ReExportException,
};

/// Refuse to rewrite the baseline in continuous integration.
pub fn refuse_in_ci() -> Result<(), String> {
    let ci = ["CI", "GITHUB_ACTIONS", "TF_BUILD"]
        .iter()
        .any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()));
    if ci {
        return Err("the baseline is never rewritten in CI; run this locally and commit the reviewed result".into());
    }
    Ok(())
}

fn budget(a: &Analysis, domain: &str) -> Budget {
    let d = &a.domains[domain];
    Budget {
        domains: d.fan_out,
        symbols: None,
        exported: d.exported,
    }
}

/// Drop exceptions that no longer occur and lower every budget to the measurement.
pub fn ratchet(a: &Analysis, symbol_domains: &[String], old: &Baseline) -> Baseline {
    let mut new = old.clone();
    let edges: BTreeSet<(&str, &str)> = a
        .edges
        .iter()
        .filter(|e| e.policy == "forbidden")
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    new.forbidden_edges
        .retain(|e| edges.contains(&(e.from.as_str(), e.to.as_str())));
    let muts: BTreeSet<(&str, &str)> = a
        .repo_tab
        .cross_mutations
        .iter()
        .map(|m| (m.domain.as_str(), m.field.as_str()))
        .collect();
    new.repo_tab_mutations
        .retain(|m| muts.contains(&(m.domain.as_str(), m.field.as_str())));
    let reexports: BTreeSet<(&str, &str, &str)> = a
        .reexports
        .iter()
        .map(|r| (r.module.as_str(), r.path.as_str(), r.target_domain.as_str()))
        .collect();
    new.reexports.retain(|r| {
        reexports.contains(&(r.module.as_str(), r.path.as_str(), r.target_domain.as_str()))
    });
    for entry in &mut new.cycles {
        let members: BTreeSet<String> = a
            .cycles
            .iter()
            .filter(|scc| scc.iter().all(|m| entry.domains.contains(m)))
            .flatten()
            .cloned()
            .collect();
        entry.edges = cycle_edges(a, &members)
            .into_iter()
            .filter(|e| entry.edges.contains(e))
            .collect();
        entry.domains = members.into_iter().collect();
    }
    new.cycles.retain(|c| !c.domains.is_empty());
    new.budgets
        .retain(|name, _| a.domains.get(name).is_some_and(|d| d.modules > 0));
    for (name, b) in &mut new.budgets {
        let measured = budget(a, name);
        b.domains = b.domains.min(measured.domains);
        b.exported = b.exported.min(measured.exported);
        if symbol_domains.contains(name) {
            b.symbols = Some(b.symbols.map_or(a.domains[name].fan_out_symbols, |s| {
                s.min(a.domains[name].fan_out_symbols)
            }));
        }
    }
    new
}

fn cycle_edges(a: &Analysis, members: &BTreeSet<String>) -> Vec<[String; 2]> {
    a.edges
        .iter()
        .filter(|e| members.contains(&e.from) && members.contains(&e.to))
        .map(|e| [e.from.clone(), e.to.clone()])
        .collect()
}

pub struct Why {
    pub reason: String,
    pub owner: String,
    pub remediation: String,
}

/// Record every current violation as an exception and raise budgets to the measurement.
pub fn adopt(a: &Analysis, symbol_domains: &[String], old: &Baseline, why: &Why) -> Baseline {
    let mut new = ratchet(a, symbol_domains, old);
    let (reason, owner, remediation) = (
        why.reason.clone(),
        why.owner.clone(),
        why.remediation.clone(),
    );
    for e in a.edges.iter().filter(|e| e.policy == "forbidden") {
        if !new
            .forbidden_edges
            .iter()
            .any(|x| x.from == e.from && x.to == e.to)
        {
            new.forbidden_edges.push(EdgeException {
                from: e.from.clone(),
                to: e.to.clone(),
                reason: reason.clone(),
                owner: owner.clone(),
                remediation: remediation.clone(),
            });
        }
    }
    for m in &a.repo_tab.cross_mutations {
        if !new
            .repo_tab_mutations
            .iter()
            .any(|x| x.domain == m.domain && x.field == m.field)
        {
            new.repo_tab_mutations.push(MutationException {
                domain: m.domain.clone(),
                field: m.field.clone(),
                reason: reason.clone(),
                owner: owner.clone(),
                remediation: remediation.clone(),
            });
        }
    }
    for r in &a.reexports {
        if !new
            .reexports
            .iter()
            .any(|x| x.module == r.module && x.path == r.path && x.target_domain == r.target_domain)
        {
            new.reexports.push(ReExportException {
                module: r.module.clone(),
                path: r.path.clone(),
                target_domain: r.target_domain.clone(),
                reason: reason.clone(),
                owner: owner.clone(),
                remediation: remediation.clone(),
            });
        }
    }
    for scc in &a.cycles {
        let members: BTreeSet<String> = scc.iter().cloned().collect();
        let edges = cycle_edges(a, &members);
        match new
            .cycles
            .iter_mut()
            .find(|c| scc.iter().all(|m| c.domains.contains(m)))
        {
            Some(entry) => entry.edges = edges,
            None => new.cycles.push(CycleException {
                domains: scc.clone(),
                edges,
                reason: reason.clone(),
                owner: owner.clone(),
                remediation: remediation.clone(),
            }),
        }
    }
    for (name, d) in a.domains.iter().filter(|(_, d)| d.modules > 0) {
        let measured = budget(a, name);
        let b = new.budgets.entry(name.clone()).or_insert(measured);
        b.domains = b.domains.max(measured.domains);
        b.exported = b.exported.max(measured.exported);
        if symbol_domains.contains(name) {
            b.symbols = Some(b.symbols.unwrap_or(0).max(d.fan_out_symbols));
        }
    }
    sort(&mut new);
    new
}

pub fn sort(b: &mut Baseline) {
    b.forbidden_edges
        .sort_by(|x, y| (&x.from, &x.to).cmp(&(&y.from, &y.to)));
    b.repo_tab_mutations
        .sort_by(|x, y| (&x.domain, &x.field).cmp(&(&y.domain, &y.field)));
    b.reexports.sort_by(|x, y| {
        (&x.module, &x.path, &x.target_domain).cmp(&(&y.module, &y.path, &y.target_domain))
    });
    for c in &mut b.cycles {
        c.domains.sort();
        c.edges.sort();
    }
    b.cycles.sort_by(|x, y| x.domains.cmp(&y.domains));
}
