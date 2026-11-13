//! Pass 2: resolved references between modules and `RepoTab` field accesses, with sites.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::collect::Chunk;
use crate::model::{Inventory, ModId, Site, VisSyntax};
use crate::resolve::{Def, Resolver};
use crate::tab::{Mode, TabInfo};
use crate::visit::Visitor;

/// How a reference was written. A re-export is a `use` with visibility beyond its module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Use,
    ReExport,
    Glob,
    Type,
    Value,
    Impl,
    Macro,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub from: ModId,
    pub to: Def,
    pub kind: RefKind,
    pub site: Site,
    pub written: String,
}

#[derive(Clone, Debug)]
pub struct Access {
    pub module: ModId,
    pub field: String,
    pub mode: Mode,
    pub site: Site,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Stats {
    /// Paths resolved to analyzed code.
    pub paths_local: usize,
    /// Paths into dependencies and std.
    pub paths_external: usize,
    /// Single-segment names not found (generic parameters, macro-made names, ...).
    pub names_unresolved: usize,
    /// Qualified paths into analyzed code that did not resolve (see gaps).
    pub paths_unresolved: usize,
    /// Method calls: their targets need type inference and are not edges.
    pub method_calls: usize,
    pub macro_bodies_parsed: usize,
    /// Macro bodies that are not expressions (`json!`, DSLs): their paths are unseen.
    pub macro_bodies_unparsed: usize,
    /// `x.field` with a `RepoTab` field name whose receiver type is not known.
    pub tab_field_unattributed: usize,
}

#[derive(Default)]
pub struct Refs {
    pub refs: Vec<Reference>,
    pub accesses: Vec<Access>,
    pub stats: Stats,
    pub unresolved: Vec<(Site, String)>,
    pub unparsed_macros: BTreeMap<String, usize>,
    pub unattributed: BTreeMap<String, usize>,
}

pub fn collect(inv: &Inventory, bodies: &[Vec<Chunk>], res: &Resolver, tab: &TabInfo) -> Refs {
    let mut out = Refs::default();
    imports(inv, res, &mut out);
    for (module, chunks) in bodies.iter().enumerate() {
        for chunk in chunks {
            let mut site = chunk.site.clone();
            site.file = chunk.file.clone();
            let mut visitor = Visitor::new(res, tab, module, site, &mut out);
            for item in &chunk.items {
                visitor.item(item);
            }
        }
    }
    out
}

/// Every module-level `use`: an import is a dependency even before its name is used, and a
/// visible one (`pub use`) widens the importing module's API.
fn imports(inv: &Inventory, res: &Resolver, out: &mut Refs) {
    for (id, import) in inv.imports.iter().enumerate() {
        if import.block {
            continue;
        }
        let kind = match (&import.name, &import.vis) {
            (None, _) => RefKind::Glob,
            (_, VisSyntax::Private | VisSyntax::SelfModule) => RefKind::Use,
            _ => RefKind::ReExport,
        };
        let written = import.path.join("::");
        let defs = res.import_any(id);
        if defs.is_empty() {
            out.stats.paths_unresolved += 1;
            out.unresolved.push((import.site.clone(), written.clone()));
        }
        for to in defs {
            if to == Def::External {
                out.stats.paths_external += 1;
                continue;
            }
            out.stats.paths_local += 1;
            out.refs.push(Reference {
                from: import.module,
                to,
                kind,
                site: import.site.clone(),
                written: written.clone(),
            });
        }
    }
}
