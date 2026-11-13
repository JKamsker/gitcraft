//! The complete, deterministic architecture inventory of one source tree.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::collect::{self, Root};
use crate::domains::{self, Edge};
use crate::model::{Gap, Inventory, ItemKind, ModId, Scope};
use crate::policy::Policy;
use crate::refs::{self, RefKind, Stats};
use crate::resolve::{Def, Resolver};
use crate::tab::TabInfo;
use crate::tab_report::{self, TabReport};

pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Default, Serialize)]
pub struct SourceInfo {
    pub revision: Option<String>,
    pub tree: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Domain {
    pub layer: String,
    pub modules: usize,
    pub files: usize,
    pub items: usize,
    /// Items whose effective visibility reaches modules of other domains.
    pub exported: usize,
    pub visibility: BTreeMap<String, usize>,
    pub fan_out: usize,
    pub fan_in: usize,
    pub fan_out_symbols: usize,
    pub fan_in_symbols: usize,
    pub test_fan_out: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReExport {
    pub domain: String,
    pub module: String,
    pub path: String,
    pub target: String,
    pub target_domain: String,
    pub file: String,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModuleFanOut {
    pub module: String,
    pub domain: String,
    pub foreign_domains: usize,
    pub foreign_symbols: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Gaps {
    pub counts: BTreeMap<String, usize>,
    /// Every gap except the high-volume kinds, which are summarized below.
    pub items: Vec<Gap>,
    pub unresolved_paths: Vec<Gap>,
    pub ambiguous_glob_names: Vec<String>,
    pub unparsed_macros: BTreeMap<String, usize>,
    pub unattributed_tab_fields: BTreeMap<String, usize>,
    pub ignored_files: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub crates: usize,
    pub files: usize,
    pub modules: usize,
    pub items: usize,
    pub domains: usize,
    pub domain_edges: usize,
    pub forbidden_edges: usize,
    pub cycles: usize,
    pub repo_tab_fields: usize,
    pub stats: Stats,
}

#[derive(Clone, Debug, Serialize)]
pub struct Analysis {
    pub schema: u32,
    pub policy_version: String,
    pub source: SourceInfo,
    pub summary: Summary,
    pub crates: BTreeMap<String, String>,
    pub domains: BTreeMap<String, Domain>,
    pub edges: Vec<Edge>,
    pub test_edges: Vec<Edge>,
    pub cycles: Vec<Vec<String>>,
    pub reexports: Vec<ReExport>,
    pub repo_tab: TabReport,
    pub module_fan_out: Vec<ModuleFanOut>,
    /// Per domain, up to ten items visible beyond it that no other domain uses.
    pub exported_unused: BTreeMap<String, Vec<String>>,
    /// Modules no domain pattern maps (an error).
    pub unmapped: Vec<String>,
    pub gaps: Gaps,
}

/// Crate roots from the policy, expanding a `*` in a root's file name.
pub fn roots(
    policy: &Policy,
    sources: &BTreeMap<String, String>,
) -> (Vec<Root>, Vec<Option<String>>) {
    let mut roots = Vec::new();
    let mut crate_domains = Vec::new();
    for spec in &policy.crates {
        let files: Vec<(String, String)> = match spec.root.split_once('*') {
            None => vec![(spec.name.clone(), spec.root.clone())],
            Some((prefix, suffix)) => sources
                .keys()
                .filter(|f| {
                    f.starts_with(prefix)
                        && f.ends_with(suffix)
                        && f.len() >= prefix.len() + suffix.len()
                })
                .filter(|f| !f[prefix.len()..f.len() - suffix.len()].contains('/'))
                .map(|f| {
                    (
                        format!("{}_{}", spec.name, &f[prefix.len()..f.len() - suffix.len()]),
                        f.clone(),
                    )
                })
                .collect(),
        };
        for (name, file) in files {
            roots.push(Root {
                name,
                file,
                externs: spec.externs.clone(),
                test: spec.test,
                cfg: spec.cfg.clone(),
            });
            crate_domains.push(spec.domain.clone());
        }
    }
    (roots, crate_domains)
}

pub fn analyze(
    sources: &BTreeMap<String, String>,
    policy: &Policy,
    source: SourceInfo,
) -> Analysis {
    let (roots, crate_domains) = roots(policy, sources);
    let collected = collect::collect(sources, &roots);
    let inv = &collected.inventory;
    let res = Resolver::new(inv);
    let tab = TabInfo::new(inv, &collected.bodies, &policy.repo_tab.path);
    let found = refs::collect(inv, &collected.bodies, &res, &tab);
    let domains = domains::map_modules(inv, policy, &crate_domains);
    let edges = domains::edges(inv, &found.refs, &domains, policy, false);
    let test_edges = domains::edges(inv, &found.refs, &domains, policy, true);
    let cycles = domains::sccs(&edges, policy);
    let owners = tab_report::owners(&res, &tab, &domains, policy);
    let repo_tab = tab_report::report(inv, &tab, &found.accesses, &domains, policy, &owners);
    let used: BTreeSet<Def> = found
        .refs
        .iter()
        .filter(|r| domains::def_module(inv, r.to).is_some_and(|t| domains[t] != domains[r.from]))
        .map(|r| r.to)
        .collect();
    let mut exported_unused = BTreeMap::new();
    let domain_reports = domain_reports(
        inv,
        &res,
        &domains,
        &edges,
        &test_edges,
        policy,
        (&used, &mut exported_unused),
    );
    let unmapped = inv
        .modules
        .iter()
        .zip(&domains)
        .filter(|(_, d)| *d == domains::UNMAPPED)
        .map(|(m, _)| m.path.clone())
        .collect();
    let reexports = reexports(inv, &found.refs, &domains);
    let module_fan_out = module_fan_out(inv, &found.refs, &domains);
    let gaps = gaps(inv, sources, policy, &found, &res);
    let summary = Summary {
        crates: inv.crates.len(),
        files: inv.files.len(),
        modules: inv.modules.len(),
        items: inv.items.len(),
        domains: domain_reports.len(),
        domain_edges: edges.len(),
        forbidden_edges: edges.iter().filter(|e| e.policy == "forbidden").count(),
        cycles: cycles.len(),
        repo_tab_fields: repo_tab.fields.len(),
        stats: found.stats.clone(),
    };
    Analysis {
        schema: SCHEMA,
        policy_version: policy.version.clone(),
        source,
        summary,
        crates: inv
            .crates
            .iter()
            .map(|c| (c.name.clone(), c.root_file.clone()))
            .collect(),
        domains: domain_reports,
        edges,
        test_edges,
        cycles,
        reexports,
        repo_tab,
        module_fan_out,
        exported_unused,
        unmapped,
        gaps,
    }
}

/// The deepest module an item is visible within, after enclosing modules' visibility, and
/// whether it is `pub` all the way to its crate root (and so visible to dependent crates).
fn effective_root(
    inv: &Inventory,
    res: &Resolver,
    module: ModId,
    vis: &crate::model::VisSyntax,
) -> (ModId, bool) {
    let root_of = |m: ModId, scope: Scope| match scope {
        Scope::Module(s) => s,
        _ => inv.crates[inv.modules[m].krate].root,
    };
    let scope = res.scope(module, vis);
    let mut public = scope == Scope::Public;
    let mut deepest = root_of(module, scope);
    let mut at = module;
    while let Some(parent) = inv.modules[at].parent {
        let scope = res.scope(parent, &inv.modules[at].vis);
        public &= scope == Scope::Public;
        let limit = root_of(parent, scope);
        if inv.is_ancestor(deepest, limit) {
            deepest = limit;
        }
        at = parent;
    }
    (deepest, public)
}

type Unused<'a> = (&'a BTreeSet<Def>, &'a mut BTreeMap<String, Vec<String>>);

fn domain_reports(
    inv: &Inventory,
    res: &Resolver,
    domains: &[String],
    edges: &[Edge],
    test_edges: &[Edge],
    policy: &Policy,
    unused: Unused,
) -> BTreeMap<String, Domain> {
    let (used, unused) = unused;
    // Domains present in each module's subtree; modules are numbered parents first.
    let mut below: Vec<BTreeSet<&str>> = domains
        .iter()
        .map(|d| BTreeSet::from([d.as_str()]))
        .collect();
    for m in (0..inv.modules.len()).rev() {
        if let Some(parent) = inv.modules[m].parent {
            let child = below[m].clone();
            below[parent].extend(child);
        }
    }
    let dependents = domains::dependent_domains(inv, domains);
    let mut out: BTreeMap<String, Domain> = BTreeMap::new();
    let entry = |out: &mut BTreeMap<String, Domain>, d: &str| -> Domain {
        out.remove(d).unwrap_or_else(|| Domain {
            layer: policy
                .domains
                .get(d)
                .map_or_else(|| "<none>".into(), |s| s.layer.clone()),
            modules: 0,
            files: 0,
            items: 0,
            exported: 0,
            visibility: BTreeMap::new(),
            fan_out: 0,
            fan_in: 0,
            fan_out_symbols: 0,
            fan_in_symbols: 0,
            test_fan_out: 0,
        })
    };
    for name in policy
        .domains
        .keys()
        .map(String::as_str)
        .chain(domains.iter().map(String::as_str))
    {
        let d = entry(&mut out, name);
        out.insert(name.to_owned(), d);
    }
    for d in domains {
        if let Some(r) = out.get_mut(d) {
            r.modules += 1;
        }
    }
    for module in inv.files.values() {
        if let Some(r) = out.get_mut(&domains[*module]) {
            r.files += 1;
        }
    }
    for (id, item) in inv.items.iter().enumerate() {
        if item.site.test
            || inv.crates[inv.modules[item.module].krate].test
            || matches!(item.kind, ItemKind::Mod(_))
        {
            continue;
        }
        let domain = &domains[item.module];
        let (root, public) = effective_root(inv, res, item.module, &item.vis);
        let krate = inv.modules[item.module].krate;
        let exported = below[root].iter().any(|d| d != domain)
            || (public && dependents[krate].iter().any(|d| d != domain));
        if exported && !used.contains(&Def::Item(id)) {
            let list = unused.entry(domain.clone()).or_default();
            if list.len() < 10 {
                list.push(format!(
                    "{} ({}:{})",
                    inv.item_path(id),
                    item.site.file,
                    item.site.line
                ));
            }
        }
        if let Some(r) = out.get_mut(domain) {
            r.items += 1;
            r.exported += usize::from(exported);
            *r.visibility.entry(item.vis.label()).or_default() += 1;
        }
    }
    let mut symbols_in: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in edges {
        if let Some(r) = out.get_mut(&e.from) {
            r.fan_out += 1;
            r.fan_out_symbols += e.symbols.len();
        }
        if let Some(r) = out.get_mut(&e.to) {
            r.fan_in += 1;
        }
        symbols_in
            .entry(&e.to)
            .or_default()
            .extend(e.symbols.keys().map(String::as_str));
    }
    for (d, symbols) in symbols_in {
        if let Some(r) = out.get_mut(d) {
            r.fan_in_symbols = symbols.len();
        }
    }
    for e in test_edges {
        if let Some(r) = out.get_mut(&e.from) {
            r.test_fan_out += 1;
        }
    }
    out
}

fn reexports(inv: &Inventory, refs: &[refs::Reference], domains: &[String]) -> Vec<ReExport> {
    let mut out: BTreeSet<ReExport> = BTreeSet::new();
    for r in refs
        .iter()
        .filter(|r| r.kind == RefKind::ReExport && !r.site.test)
    {
        let Some(target) = domains::def_module(inv, r.to) else {
            continue;
        };
        if domains[r.from] != domains[target] {
            out.insert(ReExport {
                domain: domains[r.from].clone(),
                module: inv.modules[r.from].path.clone(),
                path: r.written.clone(),
                target: domains::def_name(inv, r.to),
                target_domain: domains[target].clone(),
                file: r.site.file.clone(),
                line: r.site.line,
            });
        }
    }
    out.into_iter().collect()
}

fn module_fan_out(
    inv: &Inventory,
    refs: &[refs::Reference],
    domains: &[String],
) -> Vec<ModuleFanOut> {
    let mut map: BTreeMap<ModId, (BTreeSet<&str>, BTreeSet<String>)> = BTreeMap::new();
    for r in refs.iter().filter(|r| !r.site.test) {
        let Some(target) = domains::def_module(inv, r.to) else {
            continue;
        };
        if domains[r.from] != domains[target] {
            let entry = map.entry(r.from).or_default();
            entry.0.insert(&domains[target]);
            entry.1.insert(domains::def_name(inv, r.to));
        }
    }
    let mut out: Vec<ModuleFanOut> = map
        .into_iter()
        .map(|(m, (d, s))| ModuleFanOut {
            module: inv.modules[m].path.clone(),
            domain: domains[m].clone(),
            foreign_domains: d.len(),
            foreign_symbols: s.len(),
        })
        .collect();
    out.sort_by(|a, b| {
        (b.foreign_domains, b.foreign_symbols, &a.module).cmp(&(
            a.foreign_domains,
            a.foreign_symbols,
            &b.module,
        ))
    });
    out.truncate(30);
    out
}

fn gaps(
    inv: &Inventory,
    sources: &BTreeMap<String, String>,
    policy: &Policy,
    found: &refs::Refs,
    res: &Resolver,
) -> Gaps {
    let mut items = inv.gaps.clone();
    let mut ignored_files = Vec::new();
    for file in sources.keys().filter(|f| !inv.files.contains_key(*f)) {
        match policy
            .ignored_files
            .iter()
            .find(|i| file.starts_with(&i.path))
        {
            Some(rule) => ignored_files.push(format!("{file}: {}", rule.reason)),
            None => items.push(Gap {
                kind: "unreached-file".into(),
                file: file.clone(),
                line: 0,
                detail: "not part of any analyzed crate's module tree".into(),
            }),
        }
    }
    let mut unresolved: Vec<Gap> = found
        .unresolved
        .iter()
        .map(|(site, written)| Gap {
            kind: "unresolved-path".into(),
            file: site.file.clone(),
            line: site.line,
            detail: written.clone(),
        })
        .collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for gap in items.iter().chain(&unresolved) {
        *counts.entry(gap.kind.clone()).or_default() += 1;
    }
    let ambiguous: Vec<String> = res
        .ambiguous
        .borrow()
        .iter()
        .map(|(m, n)| format!("{m}::{n}"))
        .collect();
    counts.insert("ambiguous-glob-name".into(), ambiguous.len());
    // Item macros (`thread_local!`, ...) are counted, not listed one by one.
    items.retain(|g| g.kind != "item-macro");
    items.sort();
    unresolved.sort();
    Gaps {
        counts,
        items,
        unresolved_paths: unresolved,
        ambiguous_glob_names: ambiguous,
        unparsed_macros: found.unparsed_macros.clone(),
        unattributed_tab_fields: found.unattributed.clone(),
        ignored_files,
    }
}
