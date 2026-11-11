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
fn historical_directory_and_current_file_keep_separate_totals_and_navigation() -> Result<(), String>
{
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
