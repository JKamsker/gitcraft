//! `.dvc` files: what DVC tracks outside git (a file or folder, by its hash and size), shown as
//! the data before and after the change, the way files in Git LFS are.

use super::Sides;
use super::tree::{Field, Node};
use super::yaml;

/// One thing a `.dvc` file tracks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tracked {
    pub path: String,
    pub hash: Option<String>,
    pub size: Option<u64>,
    /// For a folder: how many files it holds.
    pub files: Option<u64>,
    /// `outs` (the data) or `deps` (where it was imported from).
    pub list: &'static str,
    pub line: u32,
}

/// What a `.dvc` file tracked before and after, paired by path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DvcDiff {
    pub pairs: Vec<(Option<Tracked>, Option<Tracked>)>,
}

pub fn is_dvc(path: &str) -> bool {
    super::extension(path) == "dvc"
}

fn scalar<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node {
        Node::Map(fields) => fields
            .iter()
            .find(|f| f.key == key)
            .and_then(|f| match &f.node {
                Node::Scalar { text, .. } => Some(text.as_str()),
                _ => None,
            }),
        _ => None,
    }
}

/// What a `.dvc` file's text tracks.
pub fn tracked(text: &str) -> Result<Vec<Tracked>, String> {
    let root = yaml::parse(text)?;
    let mut out = Vec::new();
    for list in ["outs", "deps"] {
        let Node::Map(fields) = &root else { break };
        let Some(Field {
            node: Node::List(items),
            ..
        }) = fields.iter().find(|f| f.key == list)
        else {
            continue;
        };
        for item in items {
            let n = &item.node;
            let Some(path) = scalar(n, "path") else {
                continue;
            };
            // DVC 3 writes `md5: …` with `hash: md5`; others `etag`, `checksum`.
            let hash = ["md5", "etag", "checksum", "sha256"]
                .iter()
                .find_map(|k| scalar(n, k))
                .map(str::to_owned);
            out.push(Tracked {
                path: path.to_owned(),
                hash,
                size: scalar(n, "size").and_then(|s| s.parse().ok()),
                files: scalar(n, "nfiles").and_then(|s| s.parse().ok()),
                list,
                line: item.line,
            });
        }
    }
    if out.is_empty() {
        return Err("it tracks nothing (no `outs` or `deps` with a `path`)".to_owned());
    }
    Ok(out)
}

pub fn diff(sides: &Sides) -> Result<DvcDiff, String> {
    let read = |text: &Option<String>, side: &str| -> Result<Vec<Tracked>, String> {
        text.as_deref()
            .map(|t| {
                tracked(t).map_err(|e| format!("the .dvc file {side} the change isn't valid ({e})"))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let (old, new) = (read(&sides.old, "before")?, read(&sides.new, "after")?);
    let mut pairs: Vec<(Option<Tracked>, Option<Tracked>)> = Vec::new();
    for n in &new {
        let o = old
            .iter()
            .find(|o| o.path == n.path && o.list == n.list)
            .cloned();
        pairs.push((o, Some(n.clone())));
    }
    for o in old {
        if !new.iter().any(|n| n.path == o.path && n.list == o.list) {
            pairs.push((Some(o), None));
        }
    }
    Ok(DvcDiff { pairs })
}
