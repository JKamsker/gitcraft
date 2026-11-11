//! Comparison responsibilities for rust dependencies.
use super::*;

pub(super) fn edges(function: &Function, index: &Index) -> (BTreeSet<String>, bool) {
    let mut edges = BTreeSet::new();
    let mut unresolved = function.opaque;
    for path in &function.calls {
        let Some(first) = path.first() else { continue };
        let candidate = if let Some(import) = function.imports.get(first) {
            let mut path2 = import.clone();
            path2.extend_from_slice(&path[1..]);
            Some(path2)
        } else {
            absolute(&function.module, path)
        };
        if let Some(candidate) = candidate {
            let key = candidate.join("::");
            if index.symbols.contains_key(&key) {
                edges.insert(key);
            } else {
                unresolved = true;
            }
        } else {
            unresolved = true;
        }
    }
    (edges, unresolved)
}
pub(in crate::repo::review_workflow) fn compare(old: &Index, new: &Index, path: &str) -> Report {
    let mut report = Report::default();
    report.limitations.push("Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types".into());
    report
        .limitations
        .extend(old.limitations.iter().chain(&new.limitations).cloned());
    for (name, caller) in old.symbols.iter().filter(|(_, f)| f.path == path) {
        let Some(now) = new.symbols.get(name) else {
            continue;
        };
        if caller.tokens != now.tokens {
            continue;
        }
        report.checked += 1;
        let mut queue = vec![(name.clone(), vec![name.clone()])];
        let mut seen = BTreeSet::new();
        while let Some((at, chain)) = queue.pop() {
            if !seen.insert(at.clone()) {
                continue;
            }
            if seen.len() > 1024 {
                report
                    .limitations
                    .push("Static call traversal exceeded 1024 symbols".into());
                break;
            }
            let Some(function) = old.symbols.get(&at) else {
                continue;
            };
            let (dependencies, unresolved) = edges(function, old);
            if let Some(now) = new.symbols.get(&at)
                && dependencies != edges(now, new).0
            {
                report.witnesses.push(format!(
                    "{}: static call targets changed",
                    chain.join(" → ")
                ));
            }
            if unresolved {
                report.limitations.push(
                    "Dynamic, external, local-import, or macro-generated calls remain unchecked"
                        .into(),
                );
            }
            for dependency in dependencies {
                let old_function = old.symbols.get(&dependency);
                let new_function = new.symbols.get(&dependency);
                if old_function.map(|f| &f.tokens) != new_function.map(|f| &f.tokens) {
                    report.witnesses.push(format!(
                        "{} → {} changed",
                        chain.join(" → "),
                        dependency
                    ));
                } else {
                    let mut chain = chain.clone();
                    chain.push(dependency.clone());
                    queue.push((dependency, chain));
                }
            }
        }
    }
    if old.ambiguous.iter().chain(&new.ambiguous).next().is_some() {
        report
            .limitations
            .push("Duplicate/cfg-dependent symbol identities were not resolved".into());
    }
    if report.checked == 0 {
        report
            .limitations
            .push("No unchanged uniquely identified Rust functions were analyzed".into());
    }
    report.witnesses.sort();
    report.witnesses.dedup();
    report.limitations.sort();
    report.limitations.dedup();
    report
}
