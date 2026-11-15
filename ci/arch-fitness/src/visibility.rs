//! Effective visibility follows named and glob re-export bindings, including module aliases.
//! Keep scopes separately: exporting into two sibling subtrees does not expose a third.
use std::collections::BTreeSet;

use crate::domains;
use crate::model::{Inventory, ItemKind, ModId, Ns, Scope, VisSyntax};
use crate::resolve::{Def, Resolver};

type Reach = (ModId, bool);

fn scope(inv: &Inventory, res: &Resolver, module: ModId, vis: &VisSyntax) -> Reach {
    match res.scope(module, vis) {
        Scope::Public => (inv.crates[inv.modules[module].krate].root, true),
        Scope::Crate(k) => (inv.crates[k].root, false),
        Scope::Module(m) => (m, false),
    }
}

fn intersect(inv: &Inventory, a: Reach, b: Reach) -> Option<Reach> {
    if a.1 {
        return Some(b);
    }
    if b.1 {
        return Some(a);
    }
    if inv.is_ancestor(a.0, b.0) {
        Some((b.0, a.1 && b.1))
    } else if inv.is_ancestor(b.0, a.0) {
        Some((a.0, a.1 && b.1))
    } else {
        None
    }
}

/// Resolve glob members through the resolver so private names and ambiguous globs follow
/// the same rules as ordinary references. Names include aliases and chained globs.
fn bindings(inv: &Inventory, res: &Resolver) -> Vec<(ModId, Def, Reach)> {
    let mut names: Vec<BTreeSet<String>> = inv
        .modules
        .iter()
        .map(|m| m.items.keys().chain(m.imports.keys()).cloned().collect())
        .collect();
    let mut globs = Vec::new();
    for (id, import) in inv.imports.iter().enumerate().filter(|(_, i)| !i.block) {
        if import.name.is_none() {
            for target in res.import_any(id) {
                if let Def::Mod(m) = target {
                    globs.push((id, m));
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for &(id, target) in &globs {
            let extra = names[target].clone();
            let into = &mut names[inv.imports[id].module];
            let before = into.len();
            into.extend(extra);
            changed |= before != into.len();
        }
        if !changed {
            break;
        }
    }
    let mut out = Vec::new();
    for (id, item) in inv.items.iter().enumerate().filter(|(_, i)| !i.site.test) {
        let target = match item.kind {
            ItemKind::Mod(m) => Def::Mod(m),
            _ => Def::Item(id),
        };
        out.push((item.module, target, scope(inv, res, item.module, &item.vis)));
    }
    for (id, import) in inv
        .imports
        .iter()
        .enumerate()
        .filter(|(_, i)| !i.block && !i.site.test)
    {
        let reach = scope(inv, res, import.module, &import.vis);
        if import.name.is_some() {
            for target in res.import_any(id) {
                out.push((import.module, target, reach));
            }
        } else {
            for &(_, target) in globs.iter().filter(|(glob, _)| *glob == id) {
                for name in &names[target] {
                    for ns in [Ns::Type, Ns::Value, Ns::Macro] {
                        for def in res.lookup(target, name, ns, Some(import.module)) {
                            out.push((import.module, def, reach));
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn exported(inv: &Inventory, res: &Resolver, domains: &[String]) -> BTreeSet<usize> {
    let bindings = bindings(inv, res);
    let mut modules = vec![BTreeSet::new(); inv.modules.len()];
    let mut items = vec![BTreeSet::new(); inv.items.len()];
    for krate in &inv.crates {
        modules[krate.root].insert((krate.root, true));
    }
    loop {
        let mut changed = false;
        for &(module, target, declared) in &bindings {
            for enclosing in modules[module].clone() {
                let Some(reach) = intersect(inv, enclosing, declared) else {
                    continue;
                };
                let into = match target {
                    Def::Mod(m) => &mut modules[m],
                    Def::Item(i) => &mut items[i],
                    Def::External => continue,
                };
                changed |= into.insert(reach);
            }
        }
        if !changed {
            break;
        }
    }
    let dependents = domains::dependent_domains(inv, domains);
    items
        .iter()
        .enumerate()
        .filter_map(|(id, reaches)| {
            let item = &inv.items[id];
            let owner = &domains[item.module];
            let krate = inv.modules[item.module].krate;
            reaches
                .iter()
                .any(|&(root, public)| {
                    domains
                        .iter()
                        .enumerate()
                        .any(|(m, d)| d != owner && inv.is_ancestor(root, m))
                        || (public && dependents[krate].iter().any(|d| d != owner))
                })
                .then_some(id)
        })
        .collect()
}
