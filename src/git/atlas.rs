//! A bounded immutable file tree and mailmapped change-frequency analysis.
use super::activity::{self, History, Query};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

#[derive(Default)]
pub(crate) struct Node {
    pub path: Vec<u8>,
    pub children: Vec<usize>,
    pub bytes: u64,
    pub changes: usize,
    pub contributor: String,
}
pub(crate) struct Atlas {
    pub head: String,
    pub history: Arc<History>,
    pub nodes: Vec<Node>,
    pub selected: usize,
    pub query: Query,
}
fn run(repo: &Path, args: &[&str], active: &dyn Fn() -> bool) -> Result<Vec<u8>, String> {
    activity::process::run(repo, args, None, active)
}
pub(crate) fn read(
    repo: &Path,
    limit: usize,
    query: Query,
    active: &dyn Fn() -> bool,
) -> Result<Atlas, String> {
    let history = Arc::new(activity::read(repo, limit, active, &AtomicUsize::new(0))?);
    let head = run(repo, &["rev-parse", "--revs-only", "HEAD"], active)?;
    let head = String::from_utf8(head)
        .map_err(|_| "Invalid HEAD")?
        .trim()
        .to_owned();
    let tree = if head.is_empty() {
        Vec::new()
    } else {
        run(repo, &["ls-tree", "-r", "-l", "-z", &head], active)?
    };
    build(head, history, &tree, query, active)
}
fn tree_sizes(data: &[u8]) -> Result<BTreeMap<Vec<u8>, u64>, String> {
    let mut files = BTreeMap::new();
    for entry in data.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let tab = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("Invalid tree entry")?;
        let fields: Vec<_> = entry[..tab]
            .split(|b| b.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        if fields.len() != 4 {
            return Err("Invalid tree size".into());
        }
        let size = if fields[1] == b"commit" {
            0
        } else {
            std::str::from_utf8(fields[3])
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or("Blob size unavailable (objects may be missing locally)")?
        };
        files.insert(entry[tab + 1..].to_vec(), size);
        if files.len() > 100_000 {
            return Err("Atlas is limited to 100,000 files".into());
        }
    }
    Ok(files)
}
fn build(
    head: String,
    history: Arc<History>,
    tree: &[u8],
    query: Query,
    active: &dyn Fn() -> bool,
) -> Result<Atlas, String> {
    let files = tree_sizes(tree)?;
    let mut nodes = vec![Node::default()];
    let mut paths = BTreeMap::from([(Vec::new(), 0)]);
    // Current tree files remain visible even when unchanged in the chosen period.
    for (path, size) in files {
        let id = ensure(&path, &mut nodes, &mut paths)?;
        nodes[id].bytes = size;
    }
    let mut authors: Vec<BTreeMap<(String, String), usize>> = vec![BTreeMap::new(); nodes.len()];
    let mut selected = 0;
    for record in &history.commits {
        if !active() {
            return Err("Atlas cancelled".into());
        }
        let day = record.day(query.clock);
        if query.from.is_some_and(|d| day < d) || query.through.is_some_and(|d| day > d) {
            continue;
        }
        selected += 1;
        for path in &record.paths {
            let id = ensure(path, &mut nodes, &mut paths)?;
            authors.resize_with(nodes.len(), BTreeMap::new);
            nodes[id].changes += 1;
            *authors[id]
                .entry((record.name.clone(), record.email.clone()))
                .or_default() += 1;
        }
    }
    // Parents are always allocated before children.
    for id in (0..nodes.len()).rev() {
        nodes[id].contributor = authors[id]
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|((name, email), _)| format!("{name} <{email}>"))
            .unwrap_or_default();
        if id != 0 {
            let parent = parent(&nodes[id].path);
            let at = paths[&parent];
            nodes[at].bytes = nodes[at].bytes.saturating_add(nodes[id].bytes);
            nodes[at].changes += nodes[id].changes;
            for (author, count) in std::mem::take(&mut authors[id]) {
                *authors[at].entry(author).or_default() += count;
            }
        }
    }
    Ok(Atlas {
        head,
        history,
        nodes,
        selected,
        query,
    })
}
fn parent(path: &[u8]) -> Vec<u8> {
    path.iter()
        .rposition(|b| *b == b'/')
        .map(|i| path[..i].to_vec())
        .unwrap_or_default()
}
fn ensure(
    path: &[u8],
    nodes: &mut Vec<Node>,
    paths: &mut BTreeMap<Vec<u8>, usize>,
) -> Result<usize, String> {
    if let Some(id) = paths.get(path) {
        return Ok(*id);
    }
    if path.is_empty()
        || path.len() > 32_768
        || path.split(|b| *b == b'/').count() > 128
        || nodes.len() >= 200_000
    {
        return Err("Atlas path/depth budget exceeded".into());
    }
    let parent = ensure(&parent(path), nodes, paths)?;
    let id = nodes.len();
    nodes.push(Node {
        path: path.to_vec(),
        ..Default::default()
    });
    paths.insert(path.to_vec(), id);
    nodes[parent].children.push(id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::super::activity::{Day, Record};
    use super::*;
    #[test]
    fn deleted_files_and_current_bytes_share_mailmapped_windowed_totals() -> Result<(), String> {
        let day = Day::parse("2026-11-07").ok_or("date")?;
        let records = vec![Record {
            hash: "a".repeat(40),
            name: "Canonical".into(),
            email: "a@test".into(),
            author: day,
            committer: day,
            subject: "edit".into(),
            paths: [b"src/current".to_vec(), b"src/deleted".to_vec()]
                .into_iter()
                .collect(),
        }];
        let tree = format!("100644 blob {}    80\tsrc/current\0", "b".repeat(40));
        let atlas = build(
            "head".into(),
            Arc::new(History {
                commits: records,
                ..Default::default()
            }),
            tree.as_bytes(),
            Query::default(),
            &|| true,
        )?;
        assert_eq!(atlas.nodes[0].bytes, 80);
        assert_eq!(atlas.nodes[0].changes, 2);
        assert_eq!(atlas.nodes.len(), 4);
        assert_eq!(atlas.nodes[0].contributor, "Canonical <a@test>");
        let empty = build(
            "head".into(),
            atlas.history,
            tree.as_bytes(),
            Query {
                from: Some(Day::parse("2027-01-01").ok_or("date")?),
                ..Default::default()
            },
            &|| true,
        )?;
        assert_eq!(empty.nodes[0].changes, 0);
        assert_eq!(empty.nodes[0].bytes, 80);
        assert_eq!(empty.nodes.len(), 3);
        Ok(())
    }
    #[test]
    fn parser_preserves_tabs_and_depth_is_bounded() {
        let data = format!("100644 blob {} 12\tx\ty\0", "a".repeat(40));
        assert_eq!(
            tree_sizes(data.as_bytes())
                .ok()
                .and_then(|m| m.get(b"x\ty".as_slice()).copied()),
            Some(12)
        );
        let mut nodes = vec![Node::default()];
        let mut paths = BTreeMap::from([(Vec::new(), 0)]);
        assert!(ensure("a/".repeat(200).as_bytes(), &mut nodes, &mut paths).is_err());
    }
}
