//! Module → domain mapping and the domain dependency graph built from resolved references.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::model::{Inventory, ModId};
use crate::policy::Policy;
use crate::refs::{RefKind, Reference};
use crate::resolve::Def;

pub const UNMAPPED: &str = "<unmapped>";

/// Every module's domain: the longest policy prefix, else its crate's domain.
pub fn map_modules(
    inv: &Inventory,
    policy: &Policy,
    crate_domains: &[Option<String>],
) -> Vec<String> {
    let mut patterns: Vec<(&str, &str)> = policy
        .domains
        .iter()
        .flat_map(|(name, d)| d.modules.iter().map(move |m| (m.as_str(), name.as_str())))
        .collect();
    patterns.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    inv.modules
        .iter()
        .map(|m| {
            patterns
                .iter()
                .find(|(p, _)| match p.strip_prefix('=') {
                    Some(exact) => m.path == exact,
                    None => {
                        m.path == *p
                            || m.path
                                .strip_prefix(*p)
                                .is_some_and(|rest| rest.starts_with("::"))
                    }
                })
                .map(|(_, d)| (*d).to_owned())
                .or_else(|| crate_domains.get(m.krate).cloned().flatten())
                .unwrap_or_else(|| UNMAPPED.to_owned())
        })
        .collect()
}

pub fn def_module(inv: &Inventory, def: Def) -> Option<ModId> {
    match def {
        Def::Mod(m) => Some(m),
        Def::Item(i) => Some(inv.items[i].module),
        Def::External => None,
    }
}

pub fn def_name(inv: &Inventory, def: Def) -> String {
    match def {
        Def::Mod(m) => inv.modules[m].path.clone(),
        Def::Item(i) => inv.item_path(i),
        Def::External => "<external>".into(),
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sample {
    pub file: String,
    pub line: usize,
    pub symbol: String,
    pub kind: RefKind,
    pub written: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cfg: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// `layered`, `peer`, `declared`, or `forbidden` by the policy.
    pub policy: String,
    pub references: usize,
    /// Every foreign symbol used, with its number of references and earliest site.
    pub symbols: BTreeMap<String, SymbolUse>,
    pub kinds: BTreeMap<RefKind, usize>,
    /// The earliest sites (by file and line), for diagnostics.
    pub samples: Vec<Sample>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SymbolUse {
    pub references: usize,
    pub file: String,
    pub line: usize,
}

pub const SAMPLES: usize = 5;

/// Aggregate cross-domain references; `test` selects test-only or production code.
pub fn edges(
    inv: &Inventory,
    refs: &[Reference],
    domains: &[String],
    policy: &Policy,
    test: bool,
) -> Vec<Edge> {
    type Agg = (
        usize,
        BTreeMap<String, SymbolUse>,
        BTreeMap<RefKind, usize>,
        BTreeSet<Sample>,
    );
    let mut map: BTreeMap<(String, String), Agg> = BTreeMap::new();
    for r in refs {
        let is_test = r.site.test || inv.crates[inv.modules[r.from].krate].test;
        if is_test != test {
            continue;
        }
        let Some(target) = def_module(inv, r.to) else {
            continue;
        };
        let (from, to) = (&domains[r.from], &domains[target]);
        if from == to {
            continue;
        }
        let entry = map.entry((from.clone(), to.clone())).or_default();
        entry.0 += 1;
        let symbol = def_name(inv, r.to);
        let used = entry.1.entry(symbol.clone()).or_insert_with(|| SymbolUse {
            references: 0,
            file: r.site.file.clone(),
            line: r.site.line,
        });
        used.references += 1;
        if (&r.site.file, r.site.line) < (&used.file, used.line) {
            (used.file, used.line) = (r.site.file.clone(), r.site.line);
        }
        *entry.2.entry(r.kind).or_default() += 1;
        entry.3.insert(Sample {
            file: r.site.file.clone(),
            line: r.site.line,
            symbol,
            kind: r.kind,
            written: r.written.clone(),
            cfg: r.site.cfg.clone(),
        });
        if entry.3.len() > SAMPLES {
            entry.3.pop_last();
        }
    }
    map.into_iter()
        .map(|((from, to), (references, symbols, kinds, samples))| Edge {
            policy: policy.allows(&from, &to).unwrap_or("forbidden").to_owned(),
            from,
            to,
            references,
            symbols,
            kinds,
            samples: samples.into_iter().collect(),
        })
        .collect()
}

/// Strongly connected components with more than one domain (Tarjan), sorted. Edges the
/// policy exempts as reviewed composition patterns do not take part.
pub fn sccs(edges: &[Edge], policy: &Policy) -> Vec<Vec<String>> {
    let edges: Vec<&Edge> = edges
        .iter()
        .filter(|e| policy.in_cycles(&e.from, &e.to))
        .collect();
    let nodes: BTreeSet<&str> = edges
        .iter()
        .flat_map(|e| [e.from.as_str(), e.to.as_str()])
        .collect();
    let index: BTreeMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let names: Vec<&str> = nodes.into_iter().collect();
    let mut adj = vec![Vec::new(); names.len()];
    for e in &edges {
        adj[index[e.from.as_str()]].push(index[e.to.as_str()]);
    }
    let mut t = Tarjan {
        adj: &adj,
        index: vec![None; names.len()],
        low: vec![0; names.len()],
        on_stack: vec![false; names.len()],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..names.len() {
        if t.index[v].is_none() {
            t.connect(v);
        }
    }
    let mut out: Vec<Vec<String>> = t
        .out
        .into_iter()
        .filter(|c| c.len() > 1)
        .map(|c| {
            let mut c: Vec<String> = c.into_iter().map(|i| names[i].to_owned()).collect();
            c.sort();
            c
        })
        .collect();
    out.sort();
    out
}

struct Tarjan<'a> {
    adj: &'a [Vec<usize>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    out: Vec<Vec<usize>>,
}

impl Tarjan<'_> {
    /// Iterative, so deep graphs cannot overflow the stack.
    fn connect(&mut self, root: usize) {
        let mut work = vec![(root, 0usize)];
        self.open(root);
        while let Some(&(v, at)) = work.last() {
            if let Some(&w) = self.adj[v].get(at) {
                if let Some(top) = work.last_mut() {
                    top.1 += 1;
                }
                match self.index[w] {
                    None => {
                        self.open(w);
                        work.push((w, 0));
                    }
                    Some(iw) if self.on_stack[w] => self.low[v] = self.low[v].min(iw),
                    Some(_) => {}
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                self.low[parent] = self.low[parent].min(self.low[v]);
            }
            if Some(self.low[v]) == self.index[v] {
                self.close(v);
            }
        }
    }

    fn open(&mut self, v: usize) {
        self.index[v] = Some(self.next);
        self.low[v] = self.next;
        self.next += 1;
        self.stack.push(v);
        self.on_stack[v] = true;
    }

    fn close(&mut self, v: usize) {
        let mut component = Vec::new();
        while let Some(w) = self.stack.pop() {
            self.on_stack[w] = false;
            component.push(w);
            if w == v {
                break;
            }
        }
        self.out.push(component);
    }
}

/// Per crate, the domains of the non-test crates that depend on it.
pub fn dependent_domains<'a>(inv: &Inventory, domains: &'a [String]) -> Vec<BTreeSet<&'a str>> {
    let mut out = vec![BTreeSet::new(); inv.crates.len()];
    for (m, module) in inv.modules.iter().enumerate() {
        let krate = &inv.crates[module.krate];
        if krate.test {
            continue;
        }
        for &target in krate.externs.values() {
            out[target].insert(domains[m].as_str());
        }
    }
    out
}
