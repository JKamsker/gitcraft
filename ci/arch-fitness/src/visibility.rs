//! Effective visibility follows named and glob re-export bindings, including module aliases.
//! Keep scopes separately: exporting into two sibling subtrees does not expose a third.
use std::collections::{BTreeMap, BTreeSet};

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
    // Keep visibility on each name binding, rather than on its resolved definition only.
    // A public definition can be re-exported through a narrower alias or glob binding.
    let mut members = vec![BTreeMap::<String, BTreeSet<(Def, Reach)>>::new(); inv.modules.len()];
    for (id, item) in inv.items.iter().enumerate().filter(|(_, i)| !i.site.test) {
        let target = match item.kind {
            ItemKind::Mod(m) => Def::Mod(m),
            _ => Def::Item(id),
        };
        members[item.module]
            .entry(item.name.clone())
            .or_default()
            .insert((target, scope(inv, res, item.module, &item.vis)));
    }
    let mut globs = Vec::new();
    for (id, import) in inv
        .imports
        .iter()
        .enumerate()
        .filter(|(_, i)| !i.block && !i.site.test)
    {
        let reach = scope(inv, res, import.module, &import.vis);
        for target in res.import_any(id) {
            if let Some(name) = &import.name {
                members[import.module]
                    .entry(name.clone())
                    .or_default()
                    .insert((target, reach));
            } else if let Def::Mod(m) = target {
                globs.push((import.module, m, reach));
            }
        }
    }
    loop {
        let mut changed = false;
        for &(module, target, outer) in &globs {
            for (name, bindings) in members[target].clone() {
                let visible: BTreeSet<Def> = [Ns::Type, Ns::Value, Ns::Macro]
                    .into_iter()
                    .flat_map(|ns| res.lookup(target, &name, ns, Some(module)))
                    .collect();
                for (def, source) in bindings {
                    if !visible.contains(&def) {
                        continue;
                    }
                    if let Some(reach) = intersect(inv, outer, source) {
                        changed |= members[module]
                            .entry(name.clone())
                            .or_default()
                            .insert((def, reach));
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut out = Vec::new();
    for (module, names) in members.into_iter().enumerate() {
        for bindings in names.into_values() {
            out.extend(
                bindings
                    .into_iter()
                    .map(|(def, reach)| (module, def, reach)),
            );
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
