//! Config files by key: JSON, YAML and TOML read on both sides and compared as trees, giving
//! key paths such as `server.timeout: 30 → 60`, keys added and removed, values whose type
//! changed, and keys that only moved within their map (told apart from changed values).

use std::collections::HashMap;

use super::prose::{align, fits};
use super::tree::{Field, Item, Node, path_key, shape_hash};
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
    /// The file is an OpenAPI or Swagger document, whose contract changes can be shown.
    pub api: bool,
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
                parse(t).map_err(|e| format!("the {format} {side} the change isn't valid ({e})"))
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
    // Changes in the file's order, then the keys that only moved.
    entries.sort_by_key(|e| e.change == Change::Reordered);
    let api = format != "TOML"
        && [&old, &new].iter().any(|n| {
            n.as_ref()
                .is_some_and(|n| super::api::version_of(n).is_some())
        });
    Ok(ConfigDiff {
        format,
        api,
        entries,
    })
}

/// Whether two values are the same, wherever they are.
pub fn same(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Scalar { text: x, ty: s }, Node::Scalar { text: y, ty: t }) => x == y && s == t,
        (Node::Map(x), Node::Map(y)) => {
            let (ix, iy) = (keys(x), keys(y));
            ix.len() == iy.len()
                && ix
                    .iter()
                    .all(|(k, &i)| iy.get(k).is_some_and(|&j| same(&x[i].node, &y[j].node)))
        }
        (Node::List(x), Node::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(&a.node, &b.node))
        }
        _ => false,
    }
}

/// Where each key of a map is (its last time, for a key written twice: the one parsers keep).
fn keys(fields: &[Field]) -> HashMap<&str, usize> {
    let mut index = HashMap::with_capacity(fields.len());
    for (i, f) in fields.iter().enumerate() {
        index.insert(f.key.as_str(), i);
    }
    index
}

/// A value compared by its content, its hash first.
struct Shape<'a>(u64, &'a Node);

impl PartialEq for Shape<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && same(self.1, other.1)
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
        old_line: old.map(|(_, l)| l).filter(|&l| l > 0),
        new_line: new.map(|(_, l)| l).filter(|&l| l > 0),
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

fn maps(path: &str, a: &[Field], b: &[Field], out: &mut Vec<Entry>) {
    let (ia, ib) = (keys(a), keys(b));
    // Removed keys where they were among the others: before the first key after them that
    // is still there.
    let mut removed_before: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut removed_at_end = Vec::new();
    let mut next = None;
    for (i, f) in a.iter().enumerate().rev() {
        if let Some(&j) = ib.get(f.key.as_str()) {
            // Even an earlier duplicate is an anchor for preceding removals.
            next = Some(j);
        } else if ia.get(f.key.as_str()) == Some(&i) {
            match next {
                Some(j) => removed_before.entry(j).or_default().push(i),
                None => removed_at_end.push(i),
            }
        }
    }
    for indices in removed_before.values_mut() {
        indices.reverse();
    }
    removed_at_end.reverse();
    let removed = |i: usize, out: &mut Vec<Entry>| {
        let f = &a[i];
        out.push(entry(
            &path_key(path, &f.key),
            Change::Removed,
            Some((&f.node, f.line)),
            None,
        ));
    };
    for (j, f) in b.iter().enumerate() {
        for &i in removed_before.get(&j).into_iter().flatten() {
            removed(i, out);
        }
        if ib.get(f.key.as_str()) != Some(&j) {
            continue;
        }
        let p = path_key(path, &f.key);
        match ia.get(f.key.as_str()) {
            Some(&i) => compare(
                &p,
                Some((&a[i].node, a[i].line)),
                Some((&f.node, f.line)),
                out,
            ),
            None => out.push(entry(&p, Change::Added, None, Some((&f.node, f.line)))),
        }
    }
    for i in removed_at_end {
        removed(i, out);
    }
    // Keys on both sides, in each side's order: those out of the longest run in the same order
    // moved. Only those whose value is the same are said to have moved; a changed one already
    // says so. Too many to compare: no moves are told.
    let common_old: Vec<&str> = a
        .iter()
        .enumerate()
        .filter(|(i, f)| ia.get(f.key.as_str()) == Some(i) && ib.contains_key(f.key.as_str()))
        .map(|(_, f)| f.key.as_str())
        .collect();
    let common_new: Vec<&str> = b
        .iter()
        .enumerate()
        .filter(|(j, f)| ib.get(f.key.as_str()) == Some(j) && ia.contains_key(f.key.as_str()))
        .map(|(_, f)| f.key.as_str())
        .collect();
    if !fits_middle(&common_old, &common_new) {
        return;
    }
    for (o, n) in align(&common_old, &common_new) {
        let (Some(j), None) = (n, o) else { continue };
        let key = common_new[j];
        let (Some(&i), Some(&k)) = (ia.get(key), ib.get(key)) else {
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

/// Whether what differs between the start and the end `a` and `b` share can be aligned.
fn fits_middle<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    fits(a.len() - prefix - suffix, b.len() - prefix - suffix)
}

fn lists(path: &str, a: &[Item], b: &[Item], out: &mut Vec<Entry>) {
    let at = |i: usize| format!("{path}[{i}]");
    let pair = |i: Option<usize>, j: Option<usize>, out: &mut Vec<Entry>| {
        let o = i.map(|i| (&a[i].node, a[i].line));
        let n = j.map(|j| (&b[j].node, b[j].line));
        if let Some(p) = j.or(i).map(at) {
            compare(&p, o, n, out);
        }
    };
    let sa: Vec<Shape> = a
        .iter()
        .map(|i| Shape(shape_hash(&i.node), &i.node))
        .collect();
    let sb: Vec<Shape> = b
        .iter()
        .map(|i| Shape(shape_hash(&i.node), &i.node))
        .collect();
    // What differs between the same start and end too long to align: compared by place.
    if !fits_middle(&sa, &sb) {
        for k in 0..a.len().max(b.len()) {
            pair((k < a.len()).then_some(k), (k < b.len()).then_some(k), out);
        }
        return;
    }
    let mut removed: Vec<usize> = Vec::new();
    let mut added: Vec<usize> = Vec::new();
    let flush = |removed: &mut Vec<usize>, added: &mut Vec<usize>, out: &mut Vec<Entry>| {
        // Items replaced in place are compared with what replaced them, one for one.
        for k in 0..removed.len().max(added.len()) {
            pair(removed.get(k).copied(), added.get(k).copied(), out);
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
