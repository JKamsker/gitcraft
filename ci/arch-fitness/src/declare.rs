//! Declarations of pass 1: each item's name spaces, visibility and site, and `use` trees
//! flattened into one binding per name (or glob).

use syn::Item;

use crate::collect::{Collector, Ctx};
use crate::model::{Import, Inventory, Item as Named, ItemKind, ModId, Ns, Site};
use crate::syntax::{self, line_of};

impl Collector<'_> {
    pub(crate) fn push_item(
        &mut self,
        module: ModId,
        name: &str,
        kind: ItemKind,
        ns: Vec<Ns>,
        vis: &syn::Visibility,
        site: Site,
    ) -> usize {
        let id = self.inv.items.len();
        self.inv.items.push(Named {
            module,
            name: name.to_owned(),
            kind,
            ns,
            vis: syntax::vis(vis),
            site,
            variants: Vec::new(),
            fields: Vec::new(),
            returns: None,
        });
        self.inv.modules[module]
            .items
            .entry(name.to_owned())
            .or_default()
            .push(id);
        id
    }

    pub(crate) fn register(&mut self, ctx: &Ctx, item: &Item) {
        let m = ctx.module;
        match item {
            Item::Struct(s) => {
                let site = self.child_site(ctx, &s.attrs, line_of(&s.ident));
                let ns = match s.fields {
                    syn::Fields::Named(_) => vec![Ns::Type],
                    _ => vec![Ns::Type, Ns::Value],
                };
                let id =
                    self.push_item(m, &s.ident.to_string(), ItemKind::Struct, ns, &s.vis, site);
                self.inv.items[id].fields = s
                    .fields
                    .iter()
                    .filter_map(|f| {
                        Some(crate::model::Field {
                            name: f.ident.as_ref()?.to_string(),
                            vis: syntax::vis(&f.vis),
                            line: line_of(f),
                            ty: syntax::type_head(&f.ty),
                        })
                    })
                    .collect();
            }
            Item::Enum(e) => {
                let site = self.child_site(ctx, &e.attrs, line_of(&e.ident));
                let id = self.push_item(
                    m,
                    &e.ident.to_string(),
                    ItemKind::Enum,
                    vec![Ns::Type],
                    &e.vis,
                    site,
                );
                self.inv.items[id].variants =
                    e.variants.iter().map(|v| v.ident.to_string()).collect();
            }
            Item::Fn(f) => {
                let site = self.child_site(ctx, &f.attrs, line_of(&f.sig.ident));
                let id = self.push_item(
                    m,
                    &f.sig.ident.to_string(),
                    ItemKind::Fn,
                    vec![Ns::Value],
                    &f.vis,
                    site,
                );
                self.inv.items[id].returns = syntax::return_ident(&f.sig.output);
            }
            Item::Use(u) => {
                let site = self.child_site(ctx, &u.attrs, line_of(u));
                add_use(&mut self.inv, m, u, site, false);
            }
            Item::ExternCrate(e) => {
                let site = self.child_site(ctx, &e.attrs, line_of(e));
                let name = e
                    .rename
                    .as_ref()
                    .map_or(e.ident.to_string(), |(_, r)| r.to_string());
                push_import(
                    &mut self.inv,
                    Import {
                        module: m,
                        name: Some(name),
                        path: vec![e.ident.to_string()],
                        global: true,
                        vis: syntax::vis(&e.vis),
                        site,
                        block: false,
                    },
                );
            }
            other => self.register_simple(ctx, other),
        }
    }

    pub(crate) fn register_simple(&mut self, ctx: &Ctx, item: &Item) {
        let (attrs, ident, vis, kind, ns) = match item {
            Item::Union(u) => (&u.attrs, &u.ident, &u.vis, ItemKind::Union, vec![Ns::Type]),
            Item::Trait(t) => (&t.attrs, &t.ident, &t.vis, ItemKind::Trait, vec![Ns::Type]),
            Item::TraitAlias(t) => (&t.attrs, &t.ident, &t.vis, ItemKind::Trait, vec![Ns::Type]),
            Item::Type(t) => (
                &t.attrs,
                &t.ident,
                &t.vis,
                ItemKind::TypeAlias,
                vec![Ns::Type],
            ),
            Item::Const(c) => (&c.attrs, &c.ident, &c.vis, ItemKind::Const, vec![Ns::Value]),
            Item::Static(s) => (
                &s.attrs,
                &s.ident,
                &s.vis,
                ItemKind::Static,
                vec![Ns::Value],
            ),
            Item::Macro(mac) => return self.register_macro(ctx, mac),
            _ => return,
        };
        if ident == "_" {
            return;
        }
        let site = self.child_site(ctx, attrs, line_of(ident));
        self.push_item(ctx.module, &ident.to_string(), kind, ns, vis, site);
    }

    /// A named `macro_rules!`; `#[macro_export]` also names it, public, at the crate root
    /// (`crate::name!`), while the item stays in its module for textual scope.
    fn register_macro(&mut self, ctx: &Ctx, mac: &syn::ItemMacro) {
        let line = line_of(&mac.mac);
        let Some(ident) = &mac.ident else {
            let name = syntax::tokens(&mac.mac.path);
            self.inv.gap("item-macro", &ctx.file, line, name);
            return;
        };
        let site = self.child_site(ctx, &mac.attrs, line);
        let exported = mac.attrs.iter().any(|a| a.path().is_ident("macro_export"));
        let vis = if exported {
            syn::Visibility::Public(Default::default())
        } else {
            syn::Visibility::Inherited
        };
        let name = ident.to_string();
        let id = self.push_item(
            ctx.module,
            &name,
            ItemKind::Macro,
            vec![Ns::Macro],
            &vis,
            site,
        );
        let root = self.inv.crates[self.inv.modules[ctx.module].krate].root;
        if exported && root != ctx.module {
            self.inv.modules[root]
                .items
                .entry(name)
                .or_default()
                .push(id);
        }
    }
}

/// Flatten a `use` declaration into one import per binding (or glob).
pub fn add_use(
    inv: &mut Inventory,
    module: ModId,
    u: &syn::ItemUse,
    site: Site,
    block: bool,
) -> Vec<usize> {
    let mut out = Vec::new();
    flatten(&u.tree, &mut Vec::new(), &mut out);
    out.into_iter()
        .map(|(name, path)| {
            let import = Import {
                module,
                name,
                path,
                global: u.leading_colon.is_some(),
                vis: syntax::vis(&u.vis),
                site: site.clone(),
                block,
            };
            push_import(inv, import)
        })
        .collect()
}

pub(crate) fn flatten(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Option<String>, Vec<String>)>,
) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) if n.ident == "self" => {
            out.push((prefix.last().cloned(), prefix.clone()));
        }
        syn::UseTree::Name(n) => {
            let mut path = prefix.clone();
            path.push(n.ident.to_string());
            out.push((Some(n.ident.to_string()), path));
        }
        syn::UseTree::Rename(r) => {
            let mut path = prefix.clone();
            if r.ident != "self" {
                path.push(r.ident.to_string());
            }
            out.push((Some(r.rename.to_string()), path));
        }
        syn::UseTree::Glob(_) => out.push((None, prefix.clone())),
        syn::UseTree::Group(g) => {
            for tree in &g.items {
                flatten(tree, prefix, out);
            }
        }
    }
}

pub(crate) fn push_import(inv: &mut Inventory, import: Import) -> usize {
    let id = inv.imports.len();
    let (module, name, block) = (import.module, import.name.clone(), import.block);
    inv.imports.push(import);
    if !block {
        match name {
            Some(name) => inv.modules[module]
                .imports
                .entry(name)
                .or_default()
                .push(id),
            None => inv.modules[module].globs.push(id),
        }
    }
    id
}

pub(crate) fn item_attrs(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}
