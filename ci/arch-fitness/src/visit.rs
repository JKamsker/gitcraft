//! The syntax walk of pass 2. Locals and generic parameters shadow module names; bindings
//! typed as the tab (or returned by a function that returns it) attribute field accesses.

use std::collections::HashMap;

use syn::Pat;
use syn::visit::{self, Visit};

use crate::model::{ModId, Ns, Site};
use crate::refs::{RefKind, Reference, Refs};
use crate::resolve::{Def, Resolver};
use crate::syntax::{self, line_of};
use crate::tab::{Mode, TabInfo};

/// A local binding: `Some(mutable)` when it is (a reference to) the tab.
pub(crate) type Local = Option<bool>;

#[derive(Default)]
pub(crate) struct Frame {
    locals: HashMap<String, Local>,
    uses: HashMap<String, Vec<Def>>,
}

pub(crate) enum Place {
    Plain(Mode),
    Call(String),
}

pub struct Visitor<'r, 'a> {
    pub(crate) res: &'r Resolver<'a>,
    pub(crate) tab: &'r TabInfo,
    pub(crate) module: ModId,
    pub(crate) site: Site,
    pub(crate) self_ty: Vec<Def>,
    pub(crate) self_tab: bool,
    pub(crate) frames: Vec<Frame>,
    pub(crate) ns: Ns,
    pub(crate) kind: RefKind,
    pub(crate) out: &'r mut Refs,
}

impl<'r, 'a> Visitor<'r, 'a> {
    pub fn new(
        res: &'r Resolver<'a>,
        tab: &'r TabInfo,
        module: ModId,
        site: Site,
        out: &'r mut Refs,
    ) -> Self {
        Self {
            res,
            tab,
            module,
            site,
            self_ty: Vec::new(),
            self_tab: false,
            frames: vec![Frame::default()],
            ns: Ns::Type,
            kind: RefKind::Type,
            out,
        }
    }

    pub(crate) fn at(&self, line: usize) -> Site {
        Site {
            line,
            ..self.site.clone()
        }
    }

    /// Run `f` with the cfg/test context of these attributes.
    pub(crate) fn scoped(&mut self, attrs: &[syn::Attribute], f: impl FnOnce(&mut Self)) {
        let saved = self.site.clone();
        self.site.cfg.extend(syntax::cfgs(attrs));
        self.site.test |= syntax::is_test(attrs);
        f(self);
        self.site = saved;
    }

    pub(crate) fn frame(&mut self, f: impl FnOnce(&mut Self)) {
        self.frames.push(Frame::default());
        f(self);
        self.frames.pop();
    }

    pub(crate) fn local(&self, name: &str) -> Option<Local> {
        self.frames
            .iter()
            .rev()
            .find_map(|f| f.locals.get(name).copied())
    }

    pub(crate) fn bind(&mut self, name: String, local: Local) {
        if let Some(frame) = self.frames.last_mut() {
            frame.locals.insert(name, local);
        }
    }

    pub fn item(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Use(u) => self.scoped(&u.attrs, |v| v.block_use(u)),
            syn::Item::Fn(f) => self.scoped(&f.attrs, |v| v.function(&f.sig, Some(&f.block))),
            syn::Item::Impl(i) => self.scoped(&i.attrs, |v| v.impl_block(i)),
            syn::Item::Macro(m) if m.ident.is_some() => {}
            syn::Item::Mod(m) => {
                for nested in m.content.iter().flat_map(|(_, items)| items) {
                    self.item(nested);
                }
            }
            other => self.scoped(crate::declare::item_attrs(other), |v| {
                v.frame(|v| visit::visit_item(v, other));
            }),
        }
    }

    /// A `use` inside a function body (module-level imports are resolved up front).
    fn block_use(&mut self, u: &syn::ItemUse) {
        if self.frames.len() == 1 {
            return;
        }
        let mut leaves = Vec::new();
        use_leaves(&u.tree, &mut Vec::new(), &mut leaves);
        for (name, path) in leaves {
            let mut defs = Vec::new();
            for ns in [Ns::Type, Ns::Value, Ns::Macro] {
                defs.extend(self.res.resolve(
                    self.module,
                    &path,
                    ns,
                    u.leading_colon.is_some(),
                    &self.self_ty,
                ));
            }
            defs.sort();
            defs.dedup();
            let written = path.join("::");
            self.record(&defs, RefKind::Use, line_of(u), &written);
            if let (Some(name), Some(frame)) = (name, self.frames.last_mut()) {
                frame.uses.insert(name, defs);
            }
        }
    }

    fn function(&mut self, sig: &syn::Signature, block: Option<&syn::Block>) {
        self.frame(|v| {
            v.visit_generics(&sig.generics);
            for input in &sig.inputs {
                match input {
                    syn::FnArg::Receiver(r) => {
                        let mutable = matches!(&*r.ty, syn::Type::Reference(t) if t.mutability.is_some())
                            || r.mutability.is_some();
                        v.bind("self".into(), v.self_tab.then_some(mutable));
                    }
                    syn::FnArg::Typed(t) => {
                        v.visit_type(&t.ty);
                        let typed = v.tab_type(&t.ty);
                        v.bind_pat(&t.pat, typed);
                    }
                }
            }
            v.visit_return_type(&sig.output);
            if let Some(block) = block {
                v.visit_block(block);
            }
        });
    }

    fn impl_block(&mut self, imp: &syn::ItemImpl) {
        let saved = (std::mem::take(&mut self.self_ty), self.self_tab);
        self.frame(|v| {
            v.visit_generics(&imp.generics);
            v.kind = RefKind::Impl;
            v.visit_type(&imp.self_ty);
            if let Some((_, path, _)) = &imp.trait_ {
                v.ns = Ns::Type;
                v.visit_path(path);
            }
            v.kind = RefKind::Type;
            if let syn::Type::Path(p) = &*imp.self_ty {
                let segs = syntax::path_idents(&p.path);
                v.self_ty = v.res.resolve(
                    v.module,
                    &segs,
                    Ns::Type,
                    p.path.leading_colon.is_some(),
                    &[],
                );
            }
            v.self_tab = v.is_tab(&v.self_ty);
            for item in &imp.items {
                v.impl_item(item);
            }
        });
        (self.self_ty, self.self_tab) = saved;
    }

    fn impl_item(&mut self, item: &syn::ImplItem) {
        match item {
            syn::ImplItem::Fn(f) => self.scoped(&f.attrs, |v| v.function(&f.sig, Some(&f.block))),
            syn::ImplItem::Const(c) => self.scoped(&c.attrs, |v| {
                v.visit_type(&c.ty);
                v.visit_expr(&c.expr);
            }),
            syn::ImplItem::Type(t) => self.scoped(&t.attrs, |v| v.visit_type(&t.ty)),
            syn::ImplItem::Macro(m) => self.scoped(&m.attrs, |v| v.visit_macro(&m.mac)),
            _ => {}
        }
    }

    pub(crate) fn bind_pat(&mut self, pat: &Pat, typed: Local) {
        match pat {
            Pat::Ident(i) => {
                self.bind(i.ident.to_string(), typed);
                if let Some((_, sub)) = &i.subpat {
                    self.bind_pat(sub, None);
                }
            }
            Pat::Type(t) => {
                let typed = self.tab_type(&t.ty).or(typed);
                self.bind_pat(&t.pat, typed);
            }
            Pat::Reference(r) => self.bind_pat(&r.pat, typed),
            Pat::Paren(p) => self.bind_pat(&p.pat, typed),
            Pat::Or(o) => o.cases.iter().for_each(|c| self.bind_pat(c, None)),
            Pat::Slice(s) => s.elems.iter().for_each(|e| self.bind_pat(e, None)),
            Pat::Tuple(t) => t.elems.iter().for_each(|e| self.bind_pat(e, None)),
            Pat::TupleStruct(t) => t.elems.iter().for_each(|e| self.bind_pat(e, None)),
            Pat::Struct(s) => s.fields.iter().for_each(|f| self.bind_pat(&f.pat, None)),
            _ => {}
        }
    }

    pub(crate) fn record(&mut self, defs: &[Def], kind: RefKind, line: usize, written: &str) {
        for &to in defs {
            if to == Def::External {
                self.out.stats.paths_external += 1;
                continue;
            }
            self.out.stats.paths_local += 1;
            let site = self.at(line);
            self.out.refs.push(Reference {
                from: self.module,
                to,
                kind,
                site,
                written: written.to_owned(),
            });
        }
    }

    /// What a path names here: block-scoped imports of enclosing frames, then the module.
    pub(crate) fn lookup(&self, segs: &[String], ns: Ns, global: bool) -> Vec<Def> {
        let scoped = self
            .frames
            .iter()
            .rev()
            .find_map(|f| f.uses.get(&segs[0]).cloned());
        match scoped {
            Some(start) if !global => self.res.walk(start, &segs[1..], ns),
            _ => self
                .res
                .resolve(self.module, segs, ns, global, &self.self_ty),
        }
    }

    /// Resolve and record one path; returns what it names.
    pub fn path(&mut self, path: &syn::Path, ns: Ns, kind: RefKind) -> Vec<Def> {
        let segs = syntax::path_idents(path);
        let global = path.leading_colon.is_some();
        let line = line_of(path);
        if !global && segs.len() == 1 && self.local(&segs[0]).is_some() {
            return Vec::new();
        }
        let defs = self.lookup(&segs, ns, global);
        let written = segs.join("::");
        if defs.is_empty() && kind != RefKind::Macro {
            if segs.len() > 1 || ["crate", "self", "super"].contains(&segs[0].as_str()) {
                self.out.stats.paths_unresolved += 1;
                self.out.unresolved.push((self.at(line), written.clone()));
            } else {
                self.out.stats.names_unresolved += 1;
            }
        }
        if kind != RefKind::Macro || defs.iter().any(|d| *d != Def::External) {
            self.record(&defs, kind, line, &written);
        }
        defs
    }

    pub fn ns_kind(&mut self, ns: Ns, kind: RefKind) -> (Ns, RefKind) {
        (
            std::mem::replace(&mut self.ns, ns),
            std::mem::replace(&mut self.kind, kind),
        )
    }

    pub fn restore(&mut self, saved: (Ns, RefKind)) {
        (self.ns, self.kind) = saved;
    }

    pub fn current(&self) -> (Ns, RefKind) {
        (self.ns, self.kind)
    }

    pub fn bind_local(&mut self, local: &syn::Local) {
        let typed = local
            .init
            .as_ref()
            .and_then(|init| self.tab_value(&init.expr));
        self.bind_pat(&local.pat, typed);
    }

    pub fn bind_pattern(&mut self, pat: &Pat) {
        self.bind_pat(pat, None);
    }

    pub fn bind_closure_input(&mut self, pat: &Pat) {
        self.bind_pat(pat, None);
    }

    pub fn bind_generics(&mut self, generics: &syn::Generics) {
        for param in &generics.params {
            match param {
                syn::GenericParam::Type(t) => self.bind(t.ident.to_string(), None),
                syn::GenericParam::Const(c) => self.bind(c.ident.to_string(), None),
                syn::GenericParam::Lifetime(_) => {}
            }
        }
    }

    pub fn with_frame(&mut self, f: impl FnOnce(&mut Self)) {
        self.frame(f);
    }

    pub fn trait_fn(&mut self, f: &syn::TraitItemFn) {
        self.scoped(&f.attrs, |v| v.function(&f.sig, f.default.as_ref()));
    }

    pub fn block_items(&mut self, block: &syn::Block) {
        for stmt in &block.stmts {
            if let syn::Stmt::Item(item) = stmt {
                match item {
                    syn::Item::Use(u) => self.scoped(&u.attrs, |v| v.block_use(u)),
                    other => {
                        if let Some(name) = item_name(other) {
                            self.bind(name, None);
                        }
                    }
                }
            }
        }
    }
}

fn item_name(item: &syn::Item) -> Option<String> {
    let ident = match item {
        syn::Item::Fn(f) => &f.sig.ident,
        syn::Item::Struct(s) => &s.ident,
        syn::Item::Enum(e) => &e.ident,
        syn::Item::Const(c) => &c.ident,
        syn::Item::Static(s) => &s.ident,
        syn::Item::Type(t) => &t.ident,
        syn::Item::Trait(t) => &t.ident,
        syn::Item::Mod(m) => &m.ident,
        _ => return None,
    };
    Some(ident.to_string())
}

fn use_leaves(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Option<String>, Vec<String>)>,
) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            use_leaves(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) if n.ident == "self" => {
            out.push((prefix.last().cloned(), prefix.clone()))
        }
        syn::UseTree::Name(n) => out.push((
            Some(n.ident.to_string()),
            [prefix.clone(), vec![n.ident.to_string()]].concat(),
        )),
        syn::UseTree::Rename(r) => {
            let mut path = prefix.clone();
            if r.ident != "self" {
                path.push(r.ident.to_string());
            }
            out.push((Some(r.rename.to_string()), path));
        }
        syn::UseTree::Glob(_) => out.push((None, prefix.clone())),
        syn::UseTree::Group(g) => g.items.iter().for_each(|t| use_leaves(t, prefix, out)),
    }
}
