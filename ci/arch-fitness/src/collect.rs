//! Pass 1: the module tree of every crate (files, inline, `#[path]`, `include!`), its named
//! items and its `use` bindings. Item bodies are kept for the reference pass.

use std::collections::BTreeMap;

use syn::Item;

use crate::declare::item_attrs;
use crate::model::{Crate, Inventory, ItemKind, ModId, Module, Ns, Site};
use crate::syntax::{self, line_of};

/// A crate to analyze: its root file and the names it uses for other analyzed crates.
#[derive(Clone, Debug)]
pub struct Root {
    pub name: String,
    pub file: String,
    pub externs: BTreeMap<String, String>,
    pub test: bool,
    /// Configuration flags fixed for this crate; provably inactive items are skipped.
    pub cfg: BTreeMap<String, bool>,
}

/// Items of one module written in one file, with the context they inherit.
pub struct Chunk {
    pub file: String,
    pub site: Site,
    pub items: Vec<Item>,
}

pub struct Collected {
    pub inventory: Inventory,
    pub bodies: Vec<Vec<Chunk>>,
}

#[derive(Clone)]
pub(crate) struct Ctx {
    pub(crate) module: ModId,
    pub(crate) file: String,
    /// Where `mod x;` looks for `x.rs` and `x/mod.rs`.
    pub(crate) child_dir: String,
    /// What a `#[path]` attribute is relative to.
    pub(crate) path_dir: String,
    pub(crate) site: Site,
}

pub(crate) struct Collector<'a> {
    pub(crate) sources: &'a BTreeMap<String, String>,
    pub(crate) cfg: BTreeMap<String, bool>,
    pub(crate) inv: Inventory,
    pub(crate) bodies: Vec<Vec<Chunk>>,
}

pub fn collect(sources: &BTreeMap<String, String>, roots: &[Root]) -> Collected {
    let mut c = Collector {
        sources,
        cfg: BTreeMap::new(),
        inv: Inventory::default(),
        bodies: Vec::new(),
    };
    for root in roots {
        c.add_crate(root);
    }
    let names: BTreeMap<String, usize> = c
        .inv
        .crates
        .iter()
        .enumerate()
        .map(|(i, k)| (k.name.clone(), i))
        .collect();
    for (index, root) in roots.iter().enumerate() {
        for (alias, target) in &root.externs {
            match names.get(target) {
                Some(id) => {
                    c.inv.crates[index].externs.insert(alias.clone(), *id);
                }
                None => c
                    .inv
                    .gap("unknown-extern-crate", &root.file, 0, target.clone()),
            }
        }
    }
    Collected {
        inventory: c.inv,
        bodies: c.bodies,
    }
}

impl Collector<'_> {
    fn add_crate(&mut self, root: &Root) {
        self.cfg = root.cfg.clone();
        let krate = self.inv.crates.len();
        let site = Site {
            file: root.file.clone(),
            line: 1,
            cfg: Vec::new(),
            test: root.test,
        };
        let module = self.new_module(krate, root.name.clone(), None, &root.file, site.clone());
        self.inv.crates.push(Crate {
            name: root.name.clone(),
            root_file: root.file.clone(),
            root: module,
            externs: BTreeMap::new(),
            test: root.test,
        });
        let dir = syntax::parent_dir(&root.file);
        let ctx = Ctx {
            module,
            file: root.file.clone(),
            child_dir: dir.clone(),
            path_dir: dir,
            site,
        };
        self.load_file(&ctx);
    }

    pub(crate) fn new_module(
        &mut self,
        krate: usize,
        path: String,
        parent: Option<ModId>,
        file: &str,
        site: Site,
    ) -> ModId {
        self.inv.modules.push(Module {
            krate,
            path,
            parent,
            file: file.to_owned(),
            site,
            vis: crate::model::VisSyntax::Public,
            items: BTreeMap::new(),
            imports: BTreeMap::new(),
            globs: Vec::new(),
            children: BTreeMap::new(),
        });
        self.bodies.push(Vec::new());
        self.inv.modules.len() - 1
    }

    pub(crate) fn load_file(&mut self, ctx: &Ctx) {
        let Some(source) = self.sources.get(&ctx.file) else {
            self.inv
                .gap("missing-file", &ctx.file, 0, "not a tracked Rust file");
            return;
        };
        if self
            .inv
            .files
            .insert(ctx.file.clone(), ctx.module)
            .is_some()
        {
            self.inv.gap(
                "file-reached-twice",
                &ctx.file,
                0,
                self.inv.modules[ctx.module].path.clone(),
            );
        }
        match syn::parse_file(source) {
            Ok(file) => self.walk(ctx, file.items),
            Err(e) => {
                let line = e.span().start().line;
                self.inv.gap("parse-error", &ctx.file, line, e.to_string());
            }
        }
    }

    pub(crate) fn walk(&mut self, ctx: &Ctx, items: Vec<Item>) {
        let mut keep = Vec::new();
        for item in items {
            if !syntax::cfg_active(item_attrs(&item), &self.cfg) {
                self.inv.gap(
                    "cfg-inactive",
                    &ctx.file,
                    line_of(&item),
                    "provably inactive for this crate",
                );
                continue;
            }
            match item {
                Item::Mod(m) => self.module_item(ctx, m),
                Item::Macro(m) if m.ident.is_none() && m.mac.path.is_ident("include") => {
                    self.include(ctx, &m)
                }
                other => {
                    self.register(ctx, &other);
                    keep.push(other);
                }
            }
        }
        self.bodies[ctx.module].push(Chunk {
            file: ctx.file.clone(),
            site: ctx.site.clone(),
            items: keep,
        });
    }

    pub(crate) fn include(&mut self, ctx: &Ctx, m: &syn::ItemMacro) {
        let line = line_of(&m.mac);
        let Ok(lit) = m.mac.parse_body::<syn::LitStr>() else {
            self.inv.gap(
                "include-unresolved",
                &ctx.file,
                line,
                syntax::tokens(&m.mac.tokens),
            );
            return;
        };
        let file = syntax::join(&syntax::parent_dir(&ctx.file), &lit.value());
        let included = Ctx {
            file,
            ..ctx.clone()
        };
        self.load_file(&included);
    }

    pub(crate) fn child_site(&self, ctx: &Ctx, attrs: &[syn::Attribute], line: usize) -> Site {
        let mut cfg = ctx.site.cfg.clone();
        cfg.extend(syntax::cfgs(attrs));
        Site {
            file: ctx.file.clone(),
            line,
            cfg,
            test: ctx.site.test || syntax::is_test(attrs),
        }
    }

    pub(crate) fn module_item(&mut self, ctx: &Ctx, m: syn::ItemMod) {
        let name = m.ident.to_string();
        let site = self.child_site(ctx, &m.attrs, line_of(&m.ident));
        let path = format!("{}::{}", self.inv.modules[ctx.module].path, name);
        let krate = self.inv.modules[ctx.module].krate;
        if syntax::has_cfg_attr_path(&m.attrs) {
            self.inv
                .gap("cfg-attr-path", &ctx.file, site.line, path.clone());
        }
        let path_attr = syntax::path_attr(&m.attrs);
        let (file, child_dir, path_dir) = match (&m.content, &path_attr) {
            (Some(_), Some(p)) => {
                let dir = syntax::join(&ctx.path_dir, p);
                (ctx.file.clone(), dir.clone(), dir)
            }
            (Some(_), None) => {
                let dir = syntax::join(&ctx.child_dir, &name);
                (ctx.file.clone(), dir.clone(), dir)
            }
            (None, _) => match self.module_file(ctx, &name, path_attr.as_deref(), site.line) {
                Some((file, child_dir)) => {
                    let dir = syntax::parent_dir(&file);
                    (file, child_dir, dir)
                }
                None => (String::new(), String::new(), String::new()),
            },
        };
        let id = self.new_module(krate, path, Some(ctx.module), &file, site.clone());
        self.inv.modules[id].vis = syntax::vis(&m.vis);
        self.inv.modules[ctx.module]
            .children
            .entry(name.clone())
            .or_default()
            .push(id);
        self.push_item(
            ctx.module,
            &name,
            ItemKind::Mod(id),
            vec![Ns::Type],
            &m.vis,
            site.clone(),
        );
        let child = Ctx {
            module: id,
            file: file.clone(),
            child_dir,
            path_dir,
            site,
        };
        match m.content {
            Some((_, items)) => self.walk(&child, items),
            None if !file.is_empty() => self.load_file(&child),
            None => {}
        }
    }

    /// The file of `mod name;` and the directory its own `mod` declarations use.
    fn module_file(
        &mut self,
        ctx: &Ctx,
        name: &str,
        path: Option<&str>,
        line: usize,
    ) -> Option<(String, String)> {
        if let Some(path) = path {
            // A `#[path]` file owns its directory, like `mod.rs`.
            let file = syntax::join(&ctx.path_dir, path);
            if self.sources.contains_key(&file) {
                let dir = syntax::parent_dir(&file);
                return Some((file, dir));
            }
            self.inv.gap("missing-module-file", &ctx.file, line, file);
            return None;
        }
        let flat = syntax::join(&ctx.child_dir, &format!("{name}.rs"));
        let nested = syntax::join(&ctx.child_dir, &format!("{name}/mod.rs"));
        let child_dir = syntax::join(&ctx.child_dir, name);
        match (
            self.sources.contains_key(&flat),
            self.sources.contains_key(&nested),
        ) {
            (true, true) => {
                self.inv
                    .gap("ambiguous-module-file", &ctx.file, line, flat.clone());
                Some((flat, child_dir))
            }
            (true, false) => Some((flat, child_dir)),
            (false, true) => Some((nested, child_dir)),
            (false, false) => {
                self.inv.gap("missing-module-file", &ctx.file, line, flat);
                None
            }
        }
    }
}

pub use crate::declare::add_use;
