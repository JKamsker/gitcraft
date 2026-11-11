//! A config file as a tree of keys, lists and values, each with the line it is on: what JSON,
//! YAML and TOML are read into to be compared by key.

/// What kind of value a scalar is, as the file wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    String,
    Number,
    Bool,
    Null,
    Date,
    /// A YAML alias (`*name`), compared by its name.
    Alias,
}

impl Ty {
    pub fn name(self) -> &'static str {
        match self {
            Ty::String => crate::i18n::text("repo-core-2269"),
            Ty::Number => "number",
            Ty::Bool => crate::i18n::text("repo-core-2270"),
            Ty::Null => "null",
            Ty::Date => crate::i18n::text("repo-core-2271"),
            Ty::Alias => crate::i18n::text("repo-core-2272"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Scalar { text: String, ty: Ty },
    Map(Vec<Field>),
    List(Vec<Item>),
}

/// A key of a map: its name, its value and the line it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub node: Node,
    pub line: u32,
}

/// An item of a list, and the line it starts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub node: Node,
    pub line: u32,
}

/// Values longer than this are cut when shown.
const MAX_PREVIEW: usize = 80;
/// Deeper than this, a file is refused rather than read (each level is a call).
pub const MAX_DEPTH: usize = 256;

impl Node {
    /// What kind of value it is: a scalar's type, or `map` / `list`.
    pub fn kind(&self) -> &'static str {
        match self {
            Node::Scalar { ty, .. } => ty.name(),
            Node::Map(_) => "map",
            Node::List(_) => "list",
        }
    }

    /// The value in a few words: a scalar as written (cut when long), a map or list by size.
    pub fn preview(&self) -> String {
        match self {
            Node::Scalar { text, ty } => {
                let mut s: String = text.chars().take(MAX_PREVIEW).collect();
                if text.chars().count() > MAX_PREVIEW {
                    s.push('…');
                }
                let s = s.replace('\n', "\\n");
                if *ty == Ty::String {
                    format!("\"{s}\"")
                } else {
                    s
                }
            }
            Node::Map(fields) => match fields.len() {
                0 => "{}".to_owned(),
                1 => crate::i18n::text("repo-core-2276").to_owned(),
                n => crate::i18n::message("repo-core-2277", &[("n", (n).into())]),
            },
            Node::List(items) => match items.len() {
                0 => "[]".to_owned(),
                1 => crate::i18n::text("repo-core-2278").to_owned(),
                n => crate::i18n::message("repo-core-2279", &[("n", (n).into())]),
            },
        }
    }
}

/// A hash of a value's content (not where it is), to tell values apart quickly.
pub fn shape_hash(node: &Node) -> u64 {
    use std::hash::{Hash, Hasher};
    fn feed(node: &Node, h: &mut std::collections::hash_map::DefaultHasher) {
        match node {
            Node::Scalar { text, ty } => {
                0u8.hash(h);
                text.hash(h);
                ty.name().hash(h);
            }
            Node::Map(fields) => {
                1u8.hash(h);
                // In any order: the same keys and values are the same map.
                let mut parts: Vec<u64> = fields
                    .iter()
                    .map(|f| {
                        let mut fh = std::collections::hash_map::DefaultHasher::new();
                        f.key.hash(&mut fh);
                        feed(&f.node, &mut fh);
                        fh.finish()
                    })
                    .collect();
                parts.sort_unstable();
                parts.hash(h);
            }
            Node::List(items) => {
                2u8.hash(h);
                items.len().hash(h);
                for i in items {
                    feed(&i.node, h);
                }
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    feed(node, &mut h);
    h.finish()
}

/// `key` as a step of a path: bare when it is a plain name, quoted otherwise.
pub fn path_key(parent: &str, key: &str) -> String {
    let plain = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '$' | '@' | '/'));
    let step = if plain {
        key.to_owned()
    } else {
        format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""))
    };
    if parent.is_empty() {
        step
    } else {
        format!("{parent}.{step}")
    }
}

/// Where each line of a text starts, to find the line of a byte quickly.
pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self(starts)
    }

    /// The line byte `offset` is on, from 1.
    pub fn at(&self, offset: usize) -> u32 {
        self.0.partition_point(|&s| s <= offset) as u32
    }
}

/// The line `offset` (a byte of `text`) is on, from 1.
pub fn line_at(text: &str, offset: usize) -> u32 {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}
