//! Conservative Rust static-call evidence. Parsing does not execute macros or repository
//! code. Only uniquely resolved in-repository calls produce dependency witnesses.
use crate::git;
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::{
    Item, UseTree,
    visit::{self, Visit},
};
#[derive(Clone, Default)]
pub(super) struct Report {
    pub witnesses: Vec<String>,
    pub limitations: Vec<String>,
    pub checked: usize,
}
#[derive(Default)]
pub(super) struct Index {
    symbols: BTreeMap<String, Function>,
    ambiguous: BTreeSet<String>,
    limitations: BTreeSet<String>,
}
struct Function {
    path: String,
    module: Vec<String>,
    imports: BTreeMap<String, Vec<String>>,
    tokens: String,
    calls: Vec<Vec<String>>,
    opaque: bool,
}
#[derive(Default)]
struct Calls {
    shadowed: BTreeSet<String>,
    paths: Vec<Vec<String>>,
    opaque: bool,
}
impl Calls {
    fn finish(&mut self) {
        let before = self.paths.len();
        self.paths
            .retain(|p| !p.first().is_some_and(|name| self.shadowed.contains(name)));
        self.opaque |= before != self.paths.len();
    }
}
impl<'ast> Visit<'ast> for Calls {
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.shadowed.insert(node.ident.to_string());
        visit::visit_pat_ident(self, node);
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*node.func {
            if path.qself.is_none() {
                self.paths.push(
                    path.path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect(),
                );
            } else {
                self.opaque = true;
            }
        } else {
            self.opaque = true;
        }
        visit::visit_expr_call(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.opaque = true;
        visit::visit_expr_method_call(self, node);
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.opaque = true;
    }
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {
        self.opaque = true;
    }
    fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {
        self.opaque = true;
    }
}
fn module(path: &str) -> Vec<String> {
    let (root, relative) = if let Some(relative) = path.strip_prefix("src/") {
        ("root".to_owned(), relative)
    } else if let Some((root, relative)) = path.split_once("/src/") {
        (root.replace('/', "_"), relative)
    } else {
        return vec![path.replace(['/', '.'], "_")];
    };
    let mut parts: Vec<_> = relative.split('/').map(str::to_owned).collect();
    if let Some(file) = parts.pop() {
        let stem = file.strip_suffix(".rs").unwrap_or(&file);
        if !matches!(stem, "lib" | "main" | "mod") {
            parts.push(stem.into());
        }
    }
    let mut result = vec![root];
    result.extend(parts);
    result
}
fn absolute(scope: &[String], path: &[String]) -> Option<Vec<String>> {
    if path.is_empty() {
        return None;
    }
    let mut result = scope.to_vec();
    let mut index = 0;
    if path[0] == "crate" {
        result.truncate(1);
        index = 1;
    } else if path[0] == "self" {
        index = 1;
    } else {
        while path.get(index).is_some_and(|p| p == "super") {
            if result.len() <= 1 {
                return None;
            }
            result.pop();
            index += 1;
        }
    }
    result.extend_from_slice(&path[index..]);
    Some(result)
}
fn imports(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    scope: &[String],
    out: &mut BTreeMap<String, Vec<String>>,
) {
    match tree {
        UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            imports(&p.tree, prefix, scope, out);
            prefix.pop();
        }
        UseTree::Name(n) => {
            let mut path = prefix.clone();
            if n.ident != "self" {
                path.push(n.ident.to_string());
            }
            if let Some(path) = absolute(scope, &path)
                && let Some(alias) = path.last()
            {
                out.insert(alias.clone(), path.clone());
            }
        }
        UseTree::Rename(r) => {
            let mut path = prefix.clone();
            path.push(r.ident.to_string());
            if let Some(path) = absolute(scope, &path) {
                out.insert(r.rename.to_string(), path);
            }
        }
        UseTree::Group(g) => {
            for tree in &g.items {
                imports(tree, prefix, scope, out);
            }
        }
        UseTree::Glob(_) => {}
    }
}
fn add(index: &mut Index, key: String, function: Function) {
    if index.ambiguous.contains(&key) {
        return;
    }
    if index.symbols.insert(key.clone(), function).is_some() {
        index.symbols.remove(&key);
        index.ambiguous.insert(key);
    }
}
fn scan(index: &mut Index, path: &str, items: &[Item], scope: &[String]) {
    let mut aliases = BTreeMap::new();
    for item in items {
        if let Item::Use(u) = item {
            imports(&u.tree, &mut vec![], scope, &mut aliases);
        }
    }
    for item in items {
        match item {
            Item::Fn(f) => {
                let mut calls = Calls::default();
                calls.visit_signature(&f.sig);
                calls.visit_block(&f.block);
                calls.finish();
                let key = format!("{}::{}", scope.join("::"), f.sig.ident);
                add(
                    index,
                    key,
                    Function {
                        path: path.into(),
                        module: scope.to_vec(),
                        imports: aliases.clone(),
                        tokens: f.to_token_stream().to_string(),
                        calls: calls.paths,
                        opaque: calls.opaque,
                    },
                );
            }
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    let mut nested = scope.to_vec();
                    nested.push(m.ident.to_string());
                    scan(index, path, items, &nested);
                }
            }
            Item::Impl(i) => {
                // Trait/receiver resolution needs type information. Retain a concrete inherent
                // associated function only; calls via a receiver remain explicitly unchecked.
                if i.trait_.is_some() {
                    continue;
                }
                let syn::Type::Path(ty) = &*i.self_ty else {
                    continue;
                };
                let mut owner = scope.to_vec();
                owner.extend(ty.path.segments.iter().map(|s| s.ident.to_string()));
                for item in &i.items {
                    if let syn::ImplItem::Fn(f) = item {
                        let mut calls = Calls::default();
                        calls.visit_signature(&f.sig);
                        calls.visit_block(&f.block);
                        calls.finish();
                        add(
                            index,
                            format!("{}::{}", owner.join("::"), f.sig.ident),
                            Function {
                                path: path.into(),
                                module: scope.to_vec(),
                                imports: aliases.clone(),
                                tokens: f.to_token_stream().to_string(),
                                calls: calls.paths,
                                opaque: calls.opaque,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
pub(super) fn load(repo: &Path, rev: &str) -> Result<Index, String> {
    let listing = git::run_captured(repo, &["ls-tree", "-r", "-z", "--name-only", rev], None)?;
    if listing.code != Some(0) {
        return Err(listing.stderr);
    }
    let mut index = Index::default();
    let mut paths = vec![];
    for raw in listing
        .raw
        .split(|b| *b == 0)
        .filter(|p| p.ends_with(b".rs"))
    {
        match std::str::from_utf8(raw) {
            Ok(path) if !path.contains(['\n', '\r']) => paths.push(path.to_owned()),
            _ => {
                index
                    .limitations
                    .insert("Non-UTF-8 or newline-containing Rust paths were not analyzed".into());
            }
        }
    }
    if paths.len() > 4096 {
        return Err("More than 4096 Rust files; dependency analysis was not performed".into());
    }
    if paths.is_empty() {
        return Ok(index);
    }
    let requests = paths
        .iter()
        .map(|p| format!("{rev}:{p}\n"))
        .collect::<String>();
    let batch = git::run_captured(repo, &["cat-file", "--batch"], Some(requests.as_bytes()))?;
    if batch.code != Some(0) {
        return Err(batch.stderr);
    }
    if batch.raw.len() > 32 * 1024 * 1024 {
        return Err("Rust sources exceed the 32MiB analysis budget; dependencies unchecked".into());
    }
    let mut bytes = batch.raw.as_slice();
    for path in paths {
        let newline = bytes
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("Incomplete Rust blob header")?;
        let header = std::str::from_utf8(&bytes[..newline]).map_err(|e| e.to_string())?;
        let mut parts = header.split_whitespace();
        let _ = parts.next();
        let kind = parts.next();
        let size = parts
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or("Invalid Rust blob size")?;
        if kind != Some("blob") {
            return Err("A Rust source is not a blob".into());
        }
        let source = bytes
            .get(newline + 1..newline + 1 + size)
            .ok_or("Incomplete Rust blob")?;
        bytes = bytes
            .get(newline + size + 2..)
            .ok_or("Incomplete Rust blob separator")?;
        match std::str::from_utf8(source)
            .map_err(|e| e.to_string())
            .and_then(|text| syn::parse_file(text).map_err(|e| e.to_string()))
        {
            Ok(file) => scan(&mut index, &path, &file.items, &module(&path)),
            Err(e) => {
                index
                    .limitations
                    .insert(format!("{path}: Rust syntax not parsed ({e})"));
            }
        }
    }
    Ok(index)
}
fn edges(function: &Function, index: &Index) -> (BTreeSet<String>, bool) {
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
pub(super) fn compare(old: &Index, new: &Index, path: &str) -> Report {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_rust_callers_are_invalidated_by_direct_and_transitive_callee_changes()
    -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("rust-freshness")?;
        let caller = "use crate::errors::result; pub fn caller() -> i32 { result() }\n";
        r.commit("src/caller.rs", caller, "caller")?;
        r.commit(
            "src/errors.rs",
            "pub fn result() -> i32 { leaf() } pub fn leaf() -> i32 { 1 }\n",
            "old behavior",
        )?;
        let before = r.tip("HEAD")?;
        let after = r.commit(
            "src/errors.rs",
            "pub fn result() -> i32 { leaf() } pub fn leaf() -> i32 { 2 }\n",
            "new behavior",
        )?;
        let report = compare(
            &load(r.path(), &before)?,
            &load(r.path(), &after)?,
            "src/caller.rs",
        );
        assert_eq!(report.checked, 1);
        assert!(
            report
                .witnesses
                .iter()
                .any(|s| s.contains("root::caller::caller") && s.contains("root::errors::leaf"))
        );
        assert!(
            compare(
                &load(r.path(), &before)?,
                &load(r.path(), &before)?,
                "src/caller.rs"
            )
            .witnesses
            .is_empty()
        );
        Ok(())
    }
    #[test]
    fn identical_function_names_in_other_modules_are_not_dependencies() -> Result<(), String> {
        let mut a = Index::default();
        let mut b = Index::default();
        let old=syn::parse_file("mod a { pub fn leaf()->i32 {1} pub fn caller()->i32 {leaf()} } mod b {pub fn leaf()->i32 {1}} ").map_err(|e| e.to_string())?;
        let new=syn::parse_file("mod a { pub fn leaf()->i32 {1} pub fn caller()->i32 {leaf()} } mod b {pub fn leaf()->i32 {2}} ").map_err(|e| e.to_string())?;
        scan(&mut a, "src/lib.rs", &old.items, &["root".into()]);
        scan(&mut b, "src/lib.rs", &new.items, &["root".into()]);
        assert_eq!(compare(&a, &b, "src/lib.rs").checked, 2);
        assert!(compare(&a, &b, "src/lib.rs").witnesses.is_empty());
        Ok(())
    }
    #[test]
    fn imported_call_target_changes_and_opaque_calls_are_explicit() -> Result<(), String> {
        fn index(source: &str) -> Result<Index, String> {
            let mut index = Index::default();
            let source = syn::parse_file(source).map_err(|e| e.to_string())?;
            scan(&mut index, "src/lib.rs", &source.items, &["root".into()]);
            Ok(index)
        }
        let before = index(
            "mod a {pub fn value()->i32 {1}} mod b {pub fn value()->i32 {2}} use crate::a::value; pub fn caller()->i32 {value()} pub fn opaque(x: String) {x.len();}",
        )?;
        let after = index(
            "mod a {pub fn value()->i32 {1}} mod b {pub fn value()->i32 {2}} use crate::b::value; pub fn caller()->i32 {value()} pub fn opaque(x: String) {x.len();}",
        )?;
        let report = compare(&before, &after, "src/lib.rs");
        assert!(
            report
                .witnesses
                .iter()
                .any(|s| s.contains("static call targets changed"))
        );
        assert!(report.limitations.iter().any(|s| s.contains("Dynamic")));
        Ok(())
    }
    #[test]
    fn local_bindings_are_not_mistaken_for_repository_functions() -> Result<(), String> {
        let mut before = Index::default();
        let mut after = Index::default();
        let old = syn::parse_file("fn leaf()->i32 {1} fn caller(leaf: fn()->i32)->i32 {leaf()}")
            .map_err(|e| e.to_string())?;
        let new = syn::parse_file("fn leaf()->i32 {2} fn caller(leaf: fn()->i32)->i32 {leaf()}")
            .map_err(|e| e.to_string())?;
        scan(&mut before, "src/lib.rs", &old.items, &["root".into()]);
        scan(&mut after, "src/lib.rs", &new.items, &["root".into()]);
        let report = compare(&before, &after, "src/lib.rs");
        assert!(report.witnesses.is_empty());
        assert!(!report.limitations.is_empty());
        Ok(())
    }
}
