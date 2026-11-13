//! The inventory every pass shares: crates, modules, named items and `use` bindings.

use std::collections::BTreeMap;

use serde::Serialize;

pub type ModId = usize;
pub type ItemId = usize;
pub type ImportId = usize;

/// Rust's separate name spaces; a module and a function may share a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ns {
    Type,
    Value,
    Macro,
}

/// A visibility as written, before its path is resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisSyntax {
    Private,
    Public,
    Crate,
    Super,
    SelfModule,
    InPath(Vec<String>),
}

impl VisSyntax {
    pub fn label(&self) -> String {
        match self {
            Self::Private => "private".into(),
            Self::Public => "pub".into(),
            Self::Crate => "pub(crate)".into(),
            Self::Super => "pub(super)".into(),
            Self::SelfModule => "pub(self)".into(),
            Self::InPath(path) => format!("pub(in {})", path.join("::")),
        }
    }
}

/// Where a visibility reaches: everything, one crate, or one module subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Public,
    Crate(usize),
    Module(ModId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Mod(ModId),
    Struct,
    Enum,
    Union,
    Trait,
    TypeAlias,
    Fn,
    Const,
    Static,
    Macro,
}

impl ItemKind {
    /// Items whose further path segments are associated items or variants, not names inside.
    pub fn is_type_like(&self) -> bool {
        matches!(
            self,
            Self::Struct | Self::Enum | Self::Union | Self::Trait | Self::TypeAlias
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Mod(_) => "mod",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Union => "union",
            Self::Trait => "trait",
            Self::TypeAlias => "type",
            Self::Fn => "fn",
            Self::Const => "const",
            Self::Static => "static",
            Self::Macro => "macro",
        }
    }
}

/// Where code was written, and under which compile-time conditions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Site {
    pub file: String,
    pub line: usize,
    /// `cfg` predicates of this site and every enclosing module/item, outermost first.
    pub cfg: Vec<String>,
    /// Compiled only for tests (`cfg(test)`, `#[test]`, or a test-only crate).
    pub test: bool,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub module: ModId,
    pub name: String,
    pub kind: ItemKind,
    pub ns: Vec<Ns>,
    pub vis: VisSyntax,
    pub site: Site,
    /// Variants of an enum: `use Enum::*` binds these to the enum.
    pub variants: Vec<String>,
    /// Named fields of a struct.
    pub fields: Vec<Field>,
    /// The last identifier of a function's return type, after references and wrappers.
    pub returns: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub vis: VisSyntax,
    pub line: usize,
    /// The path of the field's type after `Option`, `Box`, cells and locks.
    pub ty: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub module: ModId,
    /// The binding introduced; `None` for a glob, `Some("_")` for an anonymous import.
    pub name: Option<String>,
    pub path: Vec<String>,
    /// Written with a leading `::`.
    pub global: bool,
    pub vis: VisSyntax,
    pub site: Site,
    /// Declared inside a function body: only visible there, not a module binding.
    pub block: bool,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub krate: usize,
    /// `crate_name::a::b`, unique except for cfg-alternative declarations of one name.
    pub path: String,
    pub parent: Option<ModId>,
    pub file: String,
    pub site: Site,
    pub vis: VisSyntax,
    /// Item bindings and import bindings by name.
    pub items: BTreeMap<String, Vec<ItemId>>,
    pub imports: BTreeMap<String, Vec<ImportId>>,
    pub globs: Vec<ImportId>,
    pub children: BTreeMap<String, Vec<ModId>>,
}

#[derive(Clone, Debug)]
pub struct Crate {
    pub name: String,
    pub root_file: String,
    pub root: ModId,
    /// Names this crate may use for other analyzed crates (`gitcraft_os` → that crate).
    pub externs: BTreeMap<String, usize>,
    pub test: bool,
}

/// An explicitly reported limit of the analysis; never silently dropped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Gap {
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub detail: String,
}

#[derive(Default)]
pub struct Inventory {
    pub crates: Vec<Crate>,
    pub modules: Vec<Module>,
    pub items: Vec<Item>,
    pub imports: Vec<Import>,
    pub gaps: Vec<Gap>,
    /// Every analyzed file and the module that owns it (an `include!` maps to the includer).
    pub files: BTreeMap<String, ModId>,
}

impl Inventory {
    pub fn is_ancestor(&self, ancestor: ModId, mut module: ModId) -> bool {
        loop {
            if module == ancestor {
                return true;
            }
            match self.modules[module].parent {
                Some(parent) => module = parent,
                None => return false,
            }
        }
    }

    pub fn gap(&mut self, kind: &str, file: &str, line: usize, detail: impl Into<String>) {
        self.gaps.push(Gap {
            kind: kind.to_owned(),
            file: file.to_owned(),
            line,
            detail: detail.into(),
        });
    }

    /// The fully qualified name of an item, for reports.
    pub fn item_path(&self, item: ItemId) -> String {
        let item = &self.items[item];
        format!("{}::{}", self.modules[item.module].path, item.name)
    }
}
