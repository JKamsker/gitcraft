//! Name resolution over the inventory: uniform 2018+ paths, `crate`/`self`/`super`, aliases,
//! re-exports and glob imports (with privacy), and cfg alternatives (all are followed).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::model::{ImportId, Inventory, ItemKind, ModId, Ns, Scope, VisSyntax};

/// What a path names. `External` is anything outside the analyzed crates (std, egui, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Def {
    Mod(ModId),
    Item(usize),
    External,
}

type LookupKey = (ModId, String, Ns, Option<ModId>);

pub struct Resolver<'a> {
    pub inv: &'a Inventory,
    imports: RefCell<HashMap<(ImportId, Ns), Vec<Def>>>,
    lookups: RefCell<HashMap<LookupKey, Vec<Def>>>,
    active: RefCell<HashSet<LookupKey>>,
    active_imports: RefCell<HashSet<(ImportId, Ns)>>,
    /// Names bound by more than one glob import to different definitions.
    pub ambiguous: RefCell<BTreeSet<(String, String)>>,
}

const EXTERNAL_ROOTS: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];

impl<'a> Resolver<'a> {
    pub fn new(inv: &'a Inventory) -> Self {
        Self {
            inv,
            imports: RefCell::default(),
            lookups: RefCell::default(),
            active: RefCell::default(),
            active_imports: RefCell::default(),
            ambiguous: RefCell::default(),
        }
    }

    fn crate_root(&self, module: ModId) -> ModId {
        self.inv.crates[self.inv.modules[module].krate].root
    }

    /// Resolve a path written in `module`. `self_ty` is what `Self` names there.
    pub fn resolve(
        &self,
        module: ModId,
        segs: &[String],
        ns: Ns,
        global: bool,
        self_ty: &[Def],
    ) -> Vec<Def> {
        let Some(first) = segs.first() else {
            return Vec::new();
        };
        let start = if global {
            self.extern_crate(module, first)
        } else {
            match first.as_str() {
                "crate" => vec![Def::Mod(self.crate_root(module))],
                "self" => vec![Def::Mod(module)],
                "super" => self.inv.modules[module]
                    .parent
                    .map(Def::Mod)
                    .into_iter()
                    .collect(),
                "Self" => self_ty.to_vec(),
                name => {
                    let first_ns = if segs.len() == 1 { ns } else { Ns::Type };
                    self.lookup_scope(module, name, first_ns, segs.len() > 1)
                }
            }
        };
        self.walk(start, &segs[1..], ns)
    }

    /// Continue a resolution from known definitions (e.g. a block-scoped import).
    pub fn walk(&self, start: Vec<Def>, rest: &[String], ns: Ns) -> Vec<Def> {
        let mut current = start;
        let mut done = Vec::new();
        for (index, seg) in rest.iter().enumerate() {
            let seg_ns = if index + 1 == rest.len() {
                ns
            } else {
                Ns::Type
            };
            let mut next = Vec::new();
            for def in current {
                match def {
                    Def::Mod(m) if seg == "super" => {
                        next.extend(self.inv.modules[m].parent.map(Def::Mod))
                    }
                    Def::Mod(m) => next.extend(self.lookup(m, seg, seg_ns, None)),
                    // Associated items and variants: the dependency is on the type itself.
                    Def::Item(_) | Def::External => done.push(def),
                }
            }
            current = next;
        }
        current.extend(done);
        dedup(current)
    }

    fn extern_crate(&self, module: ModId, name: &str) -> Vec<Def> {
        let krate = &self.inv.crates[self.inv.modules[module].krate];
        match krate.externs.get(name) {
            Some(id) => vec![Def::Mod(self.inv.crates[*id].root)],
            None => vec![Def::External],
        }
    }

    /// A name at the start of a path: module bindings, then the extern prelude.
    fn lookup_scope(&self, module: ModId, name: &str, ns: Ns, qualified: bool) -> Vec<Def> {
        let found = self.lookup(module, name, ns, None);
        if !found.is_empty() {
            return found;
        }
        let krate = &self.inv.crates[self.inv.modules[module].krate];
        if let Some(id) = krate.externs.get(name) {
            return vec![Def::Mod(self.inv.crates[*id].root)];
        }
        if qualified || EXTERNAL_ROOTS.contains(&name) || is_prelude(name) {
            return vec![Def::External];
        }
        Vec::new()
    }

    /// Bindings of `name` inside `module`. `through` is the importing module of a glob,
    /// whose privacy then restricts what the glob brings in.
    pub fn lookup(&self, module: ModId, name: &str, ns: Ns, through: Option<ModId>) -> Vec<Def> {
        let key = (module, name.to_owned(), ns, through);
        if let Some(hit) = self.lookups.borrow().get(&key) {
            return hit.clone();
        }
        if !self.active.borrow_mut().insert(key.clone()) {
            return Vec::new();
        }
        let found = self.lookup_uncached(module, name, ns, through);
        self.active.borrow_mut().remove(&key);
        self.lookups.borrow_mut().insert(key, found.clone());
        found
    }

    fn lookup_uncached(
        &self,
        module: ModId,
        name: &str,
        ns: Ns,
        through: Option<ModId>,
    ) -> Vec<Def> {
        let m = &self.inv.modules[module];
        let visible = |vis: &VisSyntax| through.is_none_or(|to| self.visible(module, vis, to));
        let mut found = Vec::new();
        for &id in m.items.get(name).into_iter().flatten() {
            let item = &self.inv.items[id];
            if item.ns.contains(&ns) && visible(&item.vis) {
                found.push(match item.kind {
                    ItemKind::Mod(child) => Def::Mod(child),
                    _ => Def::Item(id),
                });
            }
        }
        for &id in m.imports.get(name).into_iter().flatten() {
            if visible(&self.inv.imports[id].vis) {
                found.extend(self.import(id, ns));
            }
        }
        if !found.is_empty() {
            return dedup(found);
        }
        self.lookup_globs(module, name, ns, through)
    }

    fn lookup_globs(&self, module: ModId, name: &str, ns: Ns, through: Option<ModId>) -> Vec<Def> {
        let mut found: Vec<Def> = Vec::new();
        let mut sources = 0;
        let mut external = false;
        for &id in &self.inv.modules[module].globs {
            let glob = &self.inv.imports[id];
            if through.is_some_and(|to| !self.visible(module, &glob.vis, to)) {
                continue;
            }
            let before = found.len();
            for source in self.import(id, Ns::Type) {
                match source {
                    Def::Mod(s) => found.extend(self.lookup(s, name, ns, Some(module))),
                    Def::Item(item) if self.inv.items[item].variants.iter().any(|v| v == name) => {
                        found.push(Def::Item(item));
                    }
                    Def::Item(_) => {}
                    Def::External => external = true,
                }
            }
            sources += usize::from(found.len() > before);
        }
        let found = dedup(found);
        if sources > 1 && found.len() > 1 {
            let path = self.inv.modules[module].path.clone();
            self.ambiguous.borrow_mut().insert((path, name.to_owned()));
        }
        if found.is_empty() && external {
            return vec![Def::External];
        }
        found
    }

    /// What an import binds in one namespace.
    pub fn import(&self, id: ImportId, ns: Ns) -> Vec<Def> {
        let key = (id, ns);
        if let Some(hit) = self.imports.borrow().get(&key) {
            return hit.clone();
        }
        if !self.active_imports.borrow_mut().insert(key) {
            return Vec::new();
        }
        let imp = &self.inv.imports[id];
        let found = self.resolve(imp.module, &imp.path, ns, imp.global, &[]);
        self.active_imports.borrow_mut().remove(&key);
        self.imports.borrow_mut().insert(key, found.clone());
        found
    }

    /// Everything one import refers to, in any namespace.
    pub fn import_any(&self, id: ImportId) -> Vec<Def> {
        let mut all: Vec<Def> = [Ns::Type, Ns::Value, Ns::Macro]
            .iter()
            .flat_map(|ns| self.import(id, *ns))
            .collect();
        if all.iter().any(|d| *d != Def::External) {
            all.retain(|d| *d != Def::External);
        }
        dedup(all)
    }

    pub fn scope(&self, declared_in: ModId, vis: &VisSyntax) -> Scope {
        let module = &self.inv.modules[declared_in];
        match vis {
            VisSyntax::Public => Scope::Public,
            VisSyntax::Crate => Scope::Crate(module.krate),
            VisSyntax::Private | VisSyntax::SelfModule => Scope::Module(declared_in),
            VisSyntax::Super => Scope::Module(module.parent.unwrap_or(declared_in)),
            VisSyntax::InPath(path) => self
                .resolve(declared_in, path, Ns::Type, false, &[])
                .into_iter()
                .find_map(|d| match d {
                    Def::Mod(m) => Some(Scope::Module(m)),
                    _ => None,
                })
                .unwrap_or(Scope::Crate(module.krate)),
        }
    }

    pub fn visible(&self, declared_in: ModId, vis: &VisSyntax, to: ModId) -> bool {
        match self.scope(declared_in, vis) {
            Scope::Public => true,
            Scope::Crate(k) => self.inv.modules[to].krate == k,
            Scope::Module(m) => self.inv.is_ancestor(m, to),
        }
    }
}

fn dedup(mut defs: Vec<Def>) -> Vec<Def> {
    defs.sort();
    defs.dedup();
    defs
}

/// Names the standard prelude and primitive types provide.
fn is_prelude(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "Option",
        "Some",
        "None",
        "Result",
        "Ok",
        "Err",
        "Vec",
        "String",
        "Box",
        "ToString",
        "ToOwned",
        "Clone",
        "Copy",
        "Send",
        "Sync",
        "Sized",
        "Unpin",
        "Drop",
        "Fn",
        "FnMut",
        "FnOnce",
        "Default",
        "Debug",
        "Eq",
        "PartialEq",
        "Ord",
        "PartialOrd",
        "Hash",
        "Iterator",
        "IntoIterator",
        "Extend",
        "From",
        "Into",
        "TryFrom",
        "TryInto",
        "AsRef",
        "AsMut",
        "DoubleEndedIterator",
        "ExactSizeIterator",
        "FromIterator",
        "drop",
        "bool",
        "char",
        "str",
        "u8",
        "u16",
        "u32",
        "u64",
        "u128",
        "usize",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "isize",
        "f32",
        "f64",
    ];
    NAMES.contains(&name)
}
