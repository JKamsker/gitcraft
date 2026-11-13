//! What the reference pass needs to attribute `RepoTab` field reads and writes: the struct,
//! its fields, functions that return it, and which method names mutate their receiver.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::collect::Chunk;
use crate::model::{Inventory, ItemKind};
use crate::syntax;

/// How code touches a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Read,
    /// Assigned (`=`, `+=`, ...), including through a nested place (`tab.f.x = ..`).
    Write,
    /// Borrowed mutably (`&mut tab.f`), e.g. handed to a function that may change it.
    MutBorrow,
    /// The receiver of a method whose name mutates in this crate or the standard library.
    MutCall,
    /// Set in a struct literal (construction).
    Init,
}

impl Mode {
    pub fn is_mutation(self) -> bool {
        matches!(self, Self::Write | Self::MutBorrow | Self::MutCall)
    }
}

/// Methods that change their receiver in std/alloc collections, `Option`, cells and locks.
const STD_MUTATING: &[&str] = &[
    "push",
    "push_back",
    "push_front",
    "pop",
    "pop_back",
    "pop_front",
    "insert",
    "remove",
    "clear",
    "truncate",
    "retain",
    "retain_mut",
    "extend",
    "append",
    "drain",
    "dedup",
    "sort",
    "sort_by",
    "sort_by_key",
    "sort_unstable",
    "reverse",
    "swap",
    "swap_remove",
    "take",
    "replace",
    "get_or_insert",
    "get_or_insert_with",
    "insert_entry",
    "entry",
    "get_mut",
    "iter_mut",
    "values_mut",
    "last_mut",
    "first_mut",
    "as_mut",
    "split_off",
    "resize",
    "borrow_mut",
    "set",
    "lock",
    "write",
    "push_str",
    "rotate_left",
    "rotate_right",
    "fill",
];

/// Methods that mutate through a shared reference (interior mutability).
const INTERIOR: &[&str] = &[
    "borrow_mut",
    "set",
    "lock",
    "write",
    "replace",
    "take",
    "swap",
];

#[derive(Default)]
pub struct TabInfo {
    pub item: Option<usize>,
    pub name: String,
    /// Field name → (visibility, line, type path).
    pub fields: BTreeMap<String, (String, usize, Vec<String>)>,
    pub module: Option<usize>,
    /// Functions and methods every definition of which returns the tab.
    pub returning: BTreeSet<String>,
    mutating: BTreeSet<String>,
}

impl TabInfo {
    /// Locate the struct `module_path::Name` and scan every impl for receiver kinds.
    pub fn new(inv: &Inventory, bodies: &[Vec<Chunk>], path: &str) -> Self {
        let mut info = Self::default();
        let found: Vec<usize> = (0..inv.items.len())
            .filter(|&id| inv.items[id].kind == ItemKind::Struct && inv.item_path(id) == path)
            .collect();
        if let [id] = found.as_slice() {
            let item = &inv.items[*id];
            info.item = Some(*id);
            info.module = Some(item.module);
            info.name = item.name.clone();
            info.fields = item
                .fields
                .iter()
                .map(|f| (f.name.clone(), (f.vis.label(), f.line, f.ty.clone())))
                .collect();
        }
        let mut receivers: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut returns: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for item in &inv.items {
            if item.kind == ItemKind::Fn {
                count_return(
                    &mut returns,
                    &item.name,
                    item.returns.as_deref() == Some(&info.name),
                );
            }
        }
        for chunk in bodies.iter().flatten() {
            for item in &chunk.items {
                scan_impl(item, &info.name, &mut receivers, &mut returns);
            }
        }
        info.mutating = receivers
            .iter()
            .filter(|(name, (mutable, shared))| {
                mutable > shared || (STD_MUTATING.contains(&name.as_str()) && *shared == 0)
            })
            .map(|(name, _)| name.clone())
            .collect();
        info.mutating.extend(
            STD_MUTATING
                .iter()
                .filter(|n| !receivers.contains_key(**n))
                .map(|n| (*n).to_owned()),
        );
        info.returning = returns
            .into_iter()
            .filter(|(_, (tab, other))| *tab > 0 && *other == 0)
            .map(|(name, _)| name)
            .collect();
        info
    }

    /// The mode of a field that is the receiver of `method`, through a binding of this mutability.
    pub fn call_mode(&self, method: &str, mutable: bool) -> Mode {
        let mutates = self.mutating.contains(method);
        if mutates && (mutable || INTERIOR.contains(&method)) {
            Mode::MutCall
        } else {
            Mode::Read
        }
    }

    pub fn is_mutating(&self, method: &str) -> bool {
        self.mutating.contains(method)
    }
}

fn count_return(map: &mut BTreeMap<String, (usize, usize)>, name: &str, tab: bool) {
    let entry = map.entry(name.to_owned()).or_default();
    if tab {
        entry.0 += 1;
    } else {
        entry.1 += 1;
    }
}

fn scan_impl(
    item: &syn::Item,
    tab: &str,
    receivers: &mut BTreeMap<String, (usize, usize)>,
    returns: &mut BTreeMap<String, (usize, usize)>,
) {
    let syn::Item::Impl(imp) = item else {
        return;
    };
    let self_is_tab = syntax::type_ident(&imp.self_ty).as_deref() == Some(tab);
    for impl_item in &imp.items {
        let syn::ImplItem::Fn(f) = impl_item else {
            continue;
        };
        let name = f.sig.ident.to_string();
        let ret = syntax::return_ident(&f.sig.output);
        let returns_tab =
            ret.as_deref() == Some(tab) || (self_is_tab && ret.as_deref() == Some("Self"));
        count_return(returns, &name, returns_tab);
        if let Some(receiver) = f.sig.receiver() {
            let mutable =
                matches!(&*receiver.ty, syn::Type::Reference(r) if r.mutability.is_some());
            let entry = receivers.entry(name).or_default();
            if mutable {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
    }
}
