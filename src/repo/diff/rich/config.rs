//! Config files by key: JSON, YAML and TOML read on both sides and compared as trees, giving
//! key paths such as `server.timeout: 30 → 60`, keys added and removed, values whose type
//! changed, and keys that only moved within their map (told apart from changed values).

use super::prose::align;
use super::tree::{Node, path_key};
use super::{Sides, extension};

/// The extensions read by key, and how.
pub const EXTENSIONS: &[&str] = &["json", "jsonc", "yaml", "yml", "toml"];

/// What happened at a key path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    Removed,
    Changed,
    /// The same key and value, in another place among its map's keys.
    Reordered,
}

/// One difference, at a key path, with where it is on each side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub change: Change,
    pub old: Option<String>,
    pub new: Option<String>,
    /// The kinds of value before and after, when the change made it another kind.
    pub kinds: Option<(&'static str, &'static str)>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

/// The change by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigDiff {
    /// `JSON`, `YAML` or `TOML`.
    pub format: &'static str,
    pub entries: Vec<Entry>,
}

/// A format's reader: the text to its tree, or what is wrong and where.
pub type Reader = fn(&str) -> Result<Node, String>;

/// The format of `path` and its reader.
pub fn format_of(path: &str) -> Option<(&'static str, Reader)> {
    match extension(path).as_str() {
        "json" | "jsonc" => Some(("JSON", super::json::parse)),
        "yaml" | "yml" => Some(("YAML", super::yaml::parse)),
        "toml" => Some(("TOML", super::toml::parse)),
        _ => None,
    }
}

/// Reads both sides as `path`'s format; an error says which side couldn't be read and why.
pub fn read(
    path: &str,
    sides: &Sides,
) -> Result<(&'static str, Option<Node>, Option<Node>), String> {
    let (format, parse) = format_of(path).ok_or("not a config file")?;
    let read = |text: &Option<String>, side: &str| -> Result<Option<Node>, String> {
        text.as_deref()
            .map(|t| {
                parse(t).map_err(|e| format!("the {format} {side} the change doesn't read: {e}"))
            })
            .transpose()
    };
    Ok((
        format,
        read(&sides.old, "before")?,
        read(&sides.new, "after")?,
    ))
}

/// Compares both sides of `path` by key.
pub fn diff(path: &str, sides: &Sides) -> Result<ConfigDiff, String> {
    let (format, old, new) = read(path, sides)?;
    let mut entries = Vec::new();
    compare(
        "",
        old.as_ref().map(|n| (n, 1)),
        new.as_ref().map(|n| (n, 1)),
        &mut entries,
    );
    Ok(ConfigDiff { format, entries })
}

/// Whether two values are the same, wherever they are.
pub fn same(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Scalar { text: x, ty: s }, Node::Scalar { text: y, ty: t }) => x == y && s == t,
        (Node::Map(x), Node::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|f| {
                    y.iter()
                        .find(|g| g.key == f.key)
                        .is_some_and(|g| same(&f.node, &g.node))
                })
        }
        (Node::List(x), Node::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(&a.node, &b.node))
        }
        _ => false,
    }
}

/// A value compared by [`same`], to align lists by.
struct Shape<'a>(&'a Node);

impl PartialEq for Shape<'_> {
    fn eq(&self, other: &Self) -> bool {
        same(self.0, other.0)
    }
}

fn entry(
    path: &str,
    change: Change,
    old: Option<(&Node, u32)>,
    new: Option<(&Node, u32)>,
) -> Entry {
    let kinds = match (old, new) {
        (Some((o, _)), Some((n, _))) if o.kind() != n.kind() => Some((o.kind(), n.kind())),
        _ => None,
    };
    Entry {
        path: if path.is_empty() {
            "(the whole file)".to_owned()
        } else {
            path.to_owned()
        },
        change,
        old: old.map(|(n, _)| n.preview()),
        new: new.map(|(n, _)| n.preview()),
        kinds,
        old_line: old.map(|(_, l)| l),
        new_line: new.map(|(_, l)| l),
    }
}

/// The differences at `path` between `old` and `new` (each a value and its line), into `out`.
fn compare(path: &str, old: Option<(&Node, u32)>, new: Option<(&Node, u32)>, out: &mut Vec<Entry>) {
    match (old, new) {
        (None, None) => {}
        (None, Some(_)) => out.push(entry(path, Change::Added, None, new)),
        (Some(_), None) => out.push(entry(path, Change::Removed, old, None)),
        (Some((Node::Map(a), _)), Some((Node::Map(b), _))) => maps(path, a, b, out),
        (Some((Node::List(a), _)), Some((Node::List(b), _))) => lists(path, a, b, out),
        (Some((a, _)), Some((b, _))) => {
            if !same(a, b) {
                out.push(entry(path, Change::Changed, old, new));
            }
        }
    }
}

fn maps(path: &str, a: &[super::tree::Field], b: &[super::tree::Field], out: &mut Vec<Entry>) {
    let find = |fields: &[super::tree::Field], key: &str| fields.iter().position(|f| f.key == key);
    for f in b {
        let p = path_key(path, &f.key);
        match find(a, &f.key) {
            Some(i) => compare(
                &p,
                Some((&a[i].node, a[i].line)),
                Some((&f.node, f.line)),
                out,
            ),
            None => out.push(entry(&p, Change::Added, None, Some((&f.node, f.line)))),
        }
    }
    for f in a.iter().filter(|f| find(b, &f.key).is_none()) {
        out.push(entry(
            &path_key(path, &f.key),
            Change::Removed,
            Some((&f.node, f.line)),
            None,
        ));
    }
    // Keys on both sides, in each side's order: those out of the longest run in the same order
    // moved. Only those whose value is the same are said to have moved; a changed one already
    // says so.
    let common_old: Vec<&str> = a
        .iter()
        .filter(|f| find(b, &f.key).is_some())
        .map(|f| f.key.as_str())
        .collect();
    let common_new: Vec<&str> = b
        .iter()
        .filter(|f| find(a, &f.key).is_some())
        .map(|f| f.key.as_str())
        .collect();
    for (o, n) in align(&common_old, &common_new) {
        let (Some(j), None) = (n, o) else { continue };
        let key = common_new[j];
        let (Some(i), Some(k)) = (find(a, key), find(b, key)) else {
            continue;
        };
        if same(&a[i].node, &b[k].node) {
            out.push(entry(
                &path_key(path, key),
                Change::Reordered,
                Some((&a[i].node, a[i].line)),
                Some((&b[k].node, b[k].line)),
            ));
        }
    }
}

fn lists(path: &str, a: &[super::tree::Item], b: &[super::tree::Item], out: &mut Vec<Entry>) {
    let sa: Vec<Shape> = a.iter().map(|i| Shape(&i.node)).collect();
    let sb: Vec<Shape> = b.iter().map(|i| Shape(&i.node)).collect();
    let at = |i: usize| format!("{path}[{i}]");
    let mut removed: Vec<usize> = Vec::new();
    let mut added: Vec<usize> = Vec::new();
    let flush = |removed: &mut Vec<usize>, added: &mut Vec<usize>, out: &mut Vec<Entry>| {
        // Items replaced in place are compared with what replaced them, one for one.
        for k in 0..removed.len().max(added.len()) {
            let o = removed.get(k).map(|&i| (&a[i].node, a[i].line));
            let n = added.get(k).map(|&j| (&b[j].node, b[j].line));
            let p = match (removed.get(k), added.get(k)) {
                (_, Some(&j)) => at(j),
                (Some(&i), None) => at(i),
                (None, None) => continue,
            };
            compare(&p, o, n, out);
        }
        removed.clear();
        added.clear();
    };
    for step in align(&sa, &sb) {
        match step {
            (Some(i), None) => removed.push(i),
            (None, Some(j)) => added.push(j),
            _ => flush(&mut removed, &mut added, out),
        }
    }
    flush(&mut removed, &mut added, out);
}
