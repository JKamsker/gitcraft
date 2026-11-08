//! A bounded immutable file tree and mailmapped change-frequency analysis.
use super::activity::{self, History, Query};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

// Conservative charges include node/map/vector overhead, both stored path copies,
// contributor buckets and bounded display labels. This is an analysis allocation
// budget, in addition to the independently bounded source streams.
const MAX_ALLOC: usize = 64 * 1024 * 1024;
const MAX_WORK: usize = 256 * 1024 * 1024;
struct Budget {
    bytes: usize,
    work: usize,
}
impl Budget {
    fn charge(&mut self, bytes: usize, work: usize) -> Result<(), String> {
        if bytes > self.bytes || work > self.work {
            return Err(crate::i18n::text(
                "backend-atlas-expanded-allocation-work-budget-exceeded-choose-a-smaller-h-74b2db",
            )
            .into());
        }
        self.bytes -= bytes;
        self.work -= work;
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Node {
    pub path: Vec<u8>,
    pub children: Vec<usize>,
    pub directory: bool,
    pub revision: Option<Arc<str>>,
    pub parent: usize,
    pub bytes: u64,
    pub changes: usize,
    pub contributor: Arc<str>,
    pub contributor_key: usize,
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
    read_observing(repo, limit, query, active, || Ok(()))
}
// Tests can change HEAD immediately after capture. Both history and bytes must
// still use this same set of immutable roots, including its captured HEAD.
pub(in crate::git) fn read_observing(
    repo: &Path,
    limit: usize,
    query: Query,
    active: &dyn Fn() -> bool,
    captured: impl FnOnce() -> Result<(), String>,
) -> Result<Atlas, String> {
    let roots = activity::capture_roots(repo, active)?;
    captured()?;
    let history = Arc::new(activity::read_roots(
        repo,
        limit,
        active,
        &AtomicUsize::new(0),
        &roots,
    )?);
    let head = roots.head;
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
            .ok_or(crate::i18n::text("backend-invalid-tree-entry-8d7b6f"))?;
        let fields: Vec<_> = entry[..tab]
            .split(|b| b.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        if fields.len() != 4 {
            return Err(crate::i18n::text("backend-invalid-tree-size-48d58f").into());
        }
        let size = if fields[1] == b"commit" {
            0
        } else {
            std::str::from_utf8(fields[3])
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or(crate::i18n::text(
                    "backend-blob-size-unavailable-objects-may-be-missing-locally-4fe3f0",
                ))?
        };
        files.insert(entry[tab + 1..].to_vec(), size);
        if files.len() > 100_000 {
            return Err(
                crate::i18n::text("backend-atlas-is-limited-to-100-000-files-386850").into(),
            );
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
    build_budget(
        head,
        history,
        tree,
        query,
        active,
        Budget {
            bytes: MAX_ALLOC,
            work: MAX_WORK,
        },
    )
}
fn build_budget(
    head: String,
    history: Arc<History>,
    tree: &[u8],
    query: Query,
    active: &dyn Fn() -> bool,
    mut budget: Budget,
) -> Result<Atlas, String> {
    let files = tree_sizes(tree)?;
    let mut nodes = vec![Node {
        directory: true,
        ..Default::default()
    }];
    let mut directories = BTreeMap::from([(Vec::new(), 0)]);
    let mut paths = BTreeMap::new();
    let head_revision: Arc<str> = head.as_str().into();
    for (path, size) in files {
        let id = ensure_file(
            &path,
            &mut nodes,
            &mut directories,
            &mut paths,
            &mut budget,
            active,
        )?;
        nodes[id].bytes = size;
        nodes[id].revision = Some(head_revision.clone());
    }
    // Full identities are borrowed once from History, never cloned per path.
    let mut identities: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut labels: Vec<(Arc<str>, usize)> = Vec::new();
    let mut authors: Vec<BTreeMap<usize, usize>> = vec![BTreeMap::new(); nodes.len()];
    let mut selected = 0;
    for record in &history.commits {
        if !active() {
            return Err(crate::i18n::text("backend-atlas-cancelled-b9165d").into());
        }
        let day = record.day(query.clock);
        if query.from.is_some_and(|d| day < d) || query.through.is_some_and(|d| day > d) {
            continue;
        }
        selected += 1;
        let identity = (record.name.as_str(), record.email.as_str());
        let author = if let Some(id) = identities.get(&identity) {
            *id
        } else {
            budget.charge(2048, record.name.len() + record.email.len())?;
            let id = labels.len();
            let key = record
                .name
                .bytes()
                .chain([0])
                .chain(record.email.bytes())
                .fold(2166136261usize, |h, b| {
                    (h ^ b as usize).wrapping_mul(16777619)
                });
            let name: String = record.name.chars().take(128).collect();
            let email: String = record.email.chars().take(96).collect();
            let shortened = name.len() != record.name.len() || email.len() != record.email.len();
            let label = if shortened {
                format!("{name} <{email}>… [contributor #{}]", id + 1)
            } else {
                format!("{name} <{email}>")
            };
            identities.insert(identity, id);
            labels.push((label.into(), key));
            id
        };
        budget.charge(128, 64)?;
        let revision: Arc<str> = record.hash.as_str().into();
        for path in &record.paths {
            let id = ensure_file(
                path,
                &mut nodes,
                &mut directories,
                &mut paths,
                &mut budget,
                active,
            )?;
            authors.resize_with(nodes.len(), BTreeMap::new);
            if nodes[id].revision.is_none() {
                nodes[id].revision = Some(revision.clone());
            }
            nodes[id].changes += 1;
            add_author(&mut authors[id], author, 1, &mut budget)?;
        }
    }
    // Folder aggregates are distinct from files at the same historical path.
    // Parents are allocated first; IDs avoid allocating transient parent prefixes.
    for id in (0..nodes.len()).rev() {
        if !active() {
            return Err(crate::i18n::text("backend-atlas-cancelled-b9165d").into());
        }
        if let Some((&author, _)) = authors[id]
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        {
            nodes[id].contributor = labels[author].0.clone();
            nodes[id].contributor_key = labels[author].1;
        }
        if id != 0 {
            let at = nodes[id].parent;
            nodes[at].bytes = nodes[at].bytes.saturating_add(nodes[id].bytes);
            nodes[at].changes += nodes[id].changes;
            for (author, count) in std::mem::take(&mut authors[id]) {
                if !active() {
                    return Err(crate::i18n::text("backend-atlas-cancelled-b9165d").into());
                }
                add_author(&mut authors[at], author, count, &mut budget)?;
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
fn add_author(
    map: &mut BTreeMap<usize, usize>,
    author: usize,
    count: usize,
    budget: &mut Budget,
) -> Result<(), String> {
    budget.charge(if map.contains_key(&author) { 0 } else { 64 }, 64)?;
    *map.entry(author).or_default() += count;
    Ok(())
}
fn parent(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|b| *b == b'/')
        .map(|i| &path[..i])
        .unwrap_or_default()
}
fn validate(path: &[u8]) -> Result<(), String> {
    if path.is_empty() || path.len() > 32_768 || path.split(|b| *b == b'/').count() > 128 {
        return Err(crate::i18n::text("backend-atlas-path-depth-budget-exceeded-4b5e68").into());
    }
    Ok(())
}
fn ensure_directory(
    path: &[u8],
    nodes: &mut Vec<Node>,
    directories: &mut BTreeMap<Vec<u8>, usize>,
    budget: &mut Budget,
    active: &dyn Fn() -> bool,
) -> Result<usize, String> {
    if !active() {
        return Err(crate::i18n::text("backend-atlas-cancelled-b9165d").into());
    }
    budget.charge(0, path.len() + 1)?;
    if let Some(id) = directories.get(path) {
        return Ok(*id);
    }
    validate(path)?;
    let parent = ensure_directory(parent(path), nodes, directories, budget, active)?;
    let id = allocate(path, true, parent, nodes, budget)?;
    directories.insert(path.to_vec(), id);
    Ok(id)
}
fn ensure_file(
    path: &[u8],
    nodes: &mut Vec<Node>,
    directories: &mut BTreeMap<Vec<u8>, usize>,
    paths: &mut BTreeMap<Vec<u8>, usize>,
    budget: &mut Budget,
    active: &dyn Fn() -> bool,
) -> Result<usize, String> {
    if !active() {
        return Err(crate::i18n::text("backend-atlas-cancelled-b9165d").into());
    }
    budget.charge(0, path.len() + 1)?;
    if let Some(id) = paths.get(path) {
        return Ok(*id);
    }
    validate(path)?;
    let parent = ensure_directory(parent(path), nodes, directories, budget, active)?;
    let id = allocate(path, false, parent, nodes, budget)?;
    paths.insert(path.to_vec(), id);
    Ok(id)
}
fn allocate(
    path: &[u8],
    directory: bool,
    parent: usize,
    nodes: &mut Vec<Node>,
    budget: &mut Budget,
) -> Result<usize, String> {
    if nodes.len() >= 200_000 {
        return Err(crate::i18n::text("backend-atlas-expanded-node-budget-exceeded-f1b1be").into());
    }
    budget.charge(2 * path.len() + 512, 1)?;
    let id = nodes.len();
    nodes.push(Node {
        path: path.to_vec(),
        directory,
        parent,
        ..Default::default()
    });
    nodes[parent].children.push(id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::super::activity::{Day, Record};
    use super::*;
    fn history_with_paths(paths: Vec<Vec<u8>>, name: String) -> Result<Arc<History>, String> {
        let day = Day::parse("2026-11-07").ok_or("date")?;
        Ok(Arc::new(History {
            commits: vec![Record {
                hash: "side".into(),
                name,
                email: "a@test".into(),
                author: day,
                committer: day,
                subject: "change".into(),
                paths: paths.into_iter().collect(),
            }],
            ..Default::default()
        }))
    }
    #[test]
    fn historical_directory_and_current_file_keep_separate_totals_and_navigation()
    -> Result<(), String> {
        let tree = format!("100644 blob {} 100\ta\0", "b".repeat(40));
        let atlas = build(
            "head".into(),
            history_with_paths(vec![b"a/old".to_vec()], "A".into())?,
            tree.as_bytes(),
            Query::default(),
            &|| true,
        )?;
        let file = atlas
            .nodes
            .iter()
            .find(|n| n.path == b"a" && !n.directory)
            .ok_or("current file")?;
        let folder = atlas
            .nodes
            .iter()
            .find(|n| n.path == b"a" && n.directory)
            .ok_or("historical folder")?;
        let historical = atlas
            .nodes
            .iter()
            .find(|n| n.path == b"a/old")
            .ok_or("historical file")?;
        assert_eq!((file.bytes, file.changes, file.children.len()), (100, 0, 0));
        assert_eq!(file.revision.as_deref(), Some("head"));
        assert_eq!(
            (folder.bytes, folder.changes, folder.children.len()),
            (0, 1, 1)
        );
        assert_eq!(historical.revision.as_deref(), Some("side"));
        assert_eq!((atlas.nodes[0].bytes, atlas.nodes[0].changes), (100, 1));
        Ok(())
    }
    #[test]
    fn expanded_path_budget_refuses_before_materializing_all_prefixes() {
        let mut nodes = vec![Node::default()];
        let mut dirs = BTreeMap::from([(Vec::new(), 0)]);
        let mut files = BTreeMap::new();
        let mut budget = Budget {
            bytes: 1800,
            work: 100_000,
        };
        let path = b"aaaa/bbbb/cccc/dddd/eeee";
        assert!(
            ensure_file(
                path,
                &mut nodes,
                &mut dirs,
                &mut files,
                &mut budget,
                &|| true,
            )
            .is_err_and(|e| e.contains("allocation/work budget"))
        );
        assert!(nodes.len() < 6);
        let calls = std::cell::Cell::new(0);
        assert!(
            ensure_file(
                path,
                &mut nodes,
                &mut dirs,
                &mut files,
                &mut Budget {
                    bytes: MAX_ALLOC,
                    work: MAX_WORK
                },
                &|| {
                    calls.set(calls.get() + 1);
                    calls.get() < 3
                }
            )
            .is_err()
        );
        assert!(nodes.len() < 6);
    }
    #[test]
    fn long_identity_is_interned_once_and_labels_share_storage() -> Result<(), String> {
        let paths = (0..512).map(|i| format!("f{i}").into_bytes()).collect();
        let history = history_with_paths(paths, "x".repeat(16_384))?;
        let atlas = build_budget(
            "head".into(),
            history,
            b"",
            Query::default(),
            &|| true,
            Budget {
                bytes: 512 * 800,
                work: 1_000_000,
            },
        )?;
        assert_eq!(atlas.nodes[0].changes, 512);
        assert!(atlas.nodes[0].contributor.len() < 1024);
        assert!(atlas.nodes[0].contributor.contains("contributor #1"));
        for n in &atlas.nodes[1..] {
            assert!(Arc::ptr_eq(&atlas.nodes[0].contributor, &n.contributor));
        }
        let small = build_budget(
            "head".into(),
            atlas.history,
            b"",
            Query::default(),
            &|| true,
            Budget {
                bytes: 1024,
                work: 1_000_000,
            },
        );
        assert!(small.is_err_and(|e| e.contains("allocation/work budget")));
        Ok(())
    }
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
        assert_eq!(atlas.nodes[0].contributor.as_ref(), "Canonical <a@test>");
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
        let mut dirs = BTreeMap::from([(Vec::new(), 0)]);
        let mut paths = BTreeMap::new();
        assert!(
            ensure_file(
                "a/".repeat(200).as_bytes(),
                &mut nodes,
                &mut dirs,
                &mut paths,
                &mut Budget {
                    bytes: MAX_ALLOC,
                    work: MAX_WORK
                },
                &|| true
            )
            .is_err()
        );
    }
}
