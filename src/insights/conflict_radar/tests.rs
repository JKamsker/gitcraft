use std::collections::{HashMap, HashSet};

use super::read::{self, FileHunks};
use super::*;
use crate::collision::Pair;
use crate::git::fixture::Repo;

const DAY: i64 = 24 * 3600;

fn candidate(name: &str, tip: &str, committed: i64, worktree: bool) -> Candidate {
    Candidate {
        name: name.to_owned(),
        tip: tip.to_owned(),
        committed,
        worktree,
    }
}

fn result(overlap: &[&str], conflicts: &[&str]) -> PairResult {
    PairResult {
        base: Some("base".to_owned()),
        changed: HashMap::new(),
        overlap: overlap.iter().map(|s| (*s).to_owned()).collect(),
        merge: Ok(Pair {
            files: conflicts.iter().map(|s| (*s).to_owned()).collect(),
        }),
        partial: None,
    }
}

#[test]
fn active_branches_are_those_in_worktrees_then_the_recent_ones() {
    let now = 100 * DAY;
    let all = vec![
        candidate("main", "m", now, true),
        candidate("old", "o", now - 90 * DAY, false),
        candidate("old-but-checked-out", "w", now - 90 * DAY, true),
        candidate("recent", "r", now - DAY, false),
        candidate("newest", "n", now, false),
    ];
    let (chosen, cut) = select(&all, Some("main"), now);
    let names: Vec<&str> = chosen.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["old-but-checked-out", "newest", "recent"]);
    assert_eq!(cut, None);
    let many: Vec<Candidate> = (0..MAX_BRANCHES + 3)
        .map(|i| candidate(&format!("b{i}"), &format!("t{i}"), now - i as i64, false))
        .collect();
    let (chosen, cut) = select(&many, None, now);
    assert_eq!(chosen.len(), MAX_BRANCHES);
    assert!(cut.is_some_and(|c| c.starts_with(&format!("{MAX_BRANCHES} of {}", MAX_BRANCHES + 3))));
}

#[test]
fn overlap_is_the_files_both_changed() {
    let a = ["x".to_owned(), "y".to_owned(), "z".to_owned()];
    let b = [
        "z".to_owned(),
        "q".to_owned(),
        "x".to_owned(),
        "x".to_owned(),
    ];
    assert_eq!(overlap(&a, &b), ["x", "z"]);
    assert!(overlap(&a, &[]).is_empty());
    let r = result(&["x", "z"], &["z"]);
    assert_eq!(r.overlap_only(), [&"x".to_owned()]);
}

#[test]
fn moving_a_tip_invalidates_exactly_its_pairs() {
    let tips: Vec<String> = ["a1", "b1", "c1", "m1"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut cache = Cache::default();
    assert_eq!(cache.missing(&tips).len(), 6);
    for (i, j) in pairs(tips.len()) {
        cache.insert(&tips[i], &tips[j], result(&[], &[]));
    }
    assert!(cache.missing(&tips).is_empty());
    // The same pair whichever way round it is asked.
    assert!(cache.get("b1", "a1").is_some());
    let mut moved = tips.clone();
    moved[1] = "b2".to_owned();
    assert_eq!(cache.missing(&moved), [(0, 1), (1, 2), (1, 3)]);
    cache.keep_only(&moved);
    assert_eq!(cache.len(), 3);
    assert!(cache.get("a1", "b1").is_none());
    assert!(cache.get("a1", "c1").is_some());
}

#[test]
fn the_landing_order_comes_from_the_pairs() {
    let nodes = vec![
        candidate("a", "a1", 0, false),
        candidate("b", "b1", 0, false),
        candidate("main", "m1", 0, false),
    ];
    let mut cache = Cache::default();
    cache.insert("a1", "b1", result(&["f"], &["f"]));
    cache.insert("a1", "m1", result(&[], &[]));
    cache.insert("b1", "m1", result(&[], &[]));
    let fresh = HashSet::from([(0, 1)]);
    let (edges, order) = assemble(nodes.clone(), true, &cache, &fresh);
    assert_eq!(edges.len(), 3);
    assert!(edges[0].fresh && !edges[1].fresh);
    assert_eq!(edges[0].weight(), 1);
    assert!(edges[0].shown() && !edges[1].shown());
    assert_eq!(order.len(), 2);
    assert!(order[0].clean());
    assert_eq!(order[1].with_earlier, [(0, 1)]);
    // Without the branch they land on, no order is made up.
    let (_, order) = assemble(nodes[..2].to_vec(), false, &cache, &fresh);
    assert!(order.is_empty());
    let radar = Radar {
        nodes,
        has_target: true,
        edges,
        ..Radar::default()
    };
    assert_eq!(radar.branches().len(), 2);
    assert_eq!(radar.target().map(|t| t.name.as_str()), Some("main"));
    let now_at = |name: &str| match name {
        "a" => Some("a1".to_owned()),
        "main" => Some("m1".to_owned()),
        _ => Some("b2".to_owned()),
    };
    let moved = radar.moved(now_at);
    assert_eq!(moved, HashSet::from([1]));
}

#[test]
fn conflict_markers_are_read_into_hunks() {
    let text = "1\n<<<<<<< a\nA\n||||||| base\n2\n=======\nB\nB2\n>>>>>>> b\n3\n<<<<<<< a\n=======\nC\n>>>>>>> b\n";
    let (hunks, more) = parse_hunks(text);
    assert!(!more);
    assert_eq!(hunks.len(), 2);
    assert_eq!(hunks[0].line, 2);
    assert_eq!(hunks[0].ours, ["A"]);
    assert_eq!(hunks[0].theirs, ["B", "B2"]);
    assert!(hunks[1].ours.is_empty());
    let long = format!(
        "<<<<<<< a\n{}=======\n>>>>>>> b\n",
        "x\n".repeat(HUNK_LINES + 5)
    );
    let (hunks, _) = parse_hunks(&long);
    assert!(hunks[0].cut);
    assert_eq!(hunks[0].ours.len(), HUNK_LINES);
    let many = "<<<<<<< a\n=======\n>>>>>>> b\n".repeat(FILE_HUNKS + 1);
    let (hunks, more) = parse_hunks(&many);
    assert_eq!(hunks.len(), FILE_HUNKS);
    assert!(more);
}

/// `main`, and three branches off it: `a` and `b` change the same line of `f.txt`; `c`
/// changes `g.txt` far from where `a` does.
fn three_branches(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(name)?;
    let g: String = (1..=12).map(|n| format!("{n}\n")).collect();
    repo.commit("base", &[("f.txt", Some("1\n2\n3\n")), ("g.txt", Some(&g))])?;
    for (branch, files) in [
        (
            "a",
            vec![
                ("f.txt", "1\nA\n3\n".to_owned()),
                ("g.txt", g.replacen("1\n", "a\n", 1)),
            ],
        ),
        ("b", vec![("f.txt", "1\nB\n3\n".to_owned())]),
        ("c", vec![("g.txt", g.replace("12\n", "c\n"))]),
    ] {
        repo.git(&["checkout", "-q", "-b", branch, "main"])?;
        let files: Vec<(&str, Option<&str>)> =
            files.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
        repo.commit(branch, &files)?;
    }
    repo.git(&["checkout", "-q", "main"])?;
    Ok(repo)
}

#[test]
fn a_real_radar_tells_conflicts_from_overlaps_and_reuses_unmoved_pairs() -> Result<(), String> {
    let repo = three_branches("radar")?;
    let now = crate::git::now();
    let (radar, cache) = read::radar(repo.path(), Cache::default(), now)?;
    assert_eq!(radar.target().map(|t| t.name.as_str()), Some("main"));
    assert_eq!(radar.branches().len(), 3);
    assert_eq!((radar.computed, radar.reused), (6, 0));
    let at = |name: &str| {
        radar
            .nodes
            .iter()
            .position(|n| n.name == name)
            .unwrap_or(99)
    };
    let (a, b, c) = (at("a"), at("b"), at("c"));
    let ab = radar.edge(a, b).ok_or("no a-b")?;
    assert_eq!(ab.result.conflicts(), ["f.txt"]);
    assert_eq!(ab.result.overlap, ["f.txt"]);
    let ac = radar.edge(a, c).ok_or("no a-c")?;
    assert!(ac.result.conflicts().is_empty());
    assert_eq!(ac.result.merge_error(), None);
    assert_eq!(ac.result.overlap, ["g.txt"]);
    let bc = radar.edge(b, c).ok_or("no b-c")?;
    assert!(!bc.shown());
    // One of the two conflicting branches lands after the other, which it needs rework for.
    assert_eq!(radar.order.len(), 3);
    assert_eq!(radar.order.iter().filter(|s| !s.clean()).count(), 1);

    // The conflicted region, as the dry-run merge writes it.
    let a_tip = &radar.nodes[a].tip;
    let b_tip = &radar.nodes[b].tip;
    let hunks = read::conflict_hunks(repo.path(), a_tip, b_tip, &["f.txt".to_owned()])?;
    let FileHunks { hunks, .. } = &hunks[0];
    let hunks = hunks.as_ref().map_err(|e| e.clone())?;
    assert_eq!(hunks.len(), 1);
    assert_eq!(
        (hunks[0].ours.as_slice(), hunks[0].theirs.as_slice()),
        (&["A".to_owned()][..], &["B".to_owned()][..])
    );

    // `c` moves: its three pairs are merged again, the other three are reused.
    repo.git(&["checkout", "-q", "c"])?;
    repo.commit("more c", &[("h.txt", Some("h\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    let c_tip = repo.rev("c")?;
    let moved = radar.moved(|name| repo.rev(name).ok());
    assert_eq!(moved, HashSet::from([c]));
    let (again, cache) = read::radar(repo.path(), cache, now)?;
    assert_eq!((again.computed, again.reused), (3, 3));
    assert_eq!(cache.len(), 6);
    for e in &again.edges {
        let has_c = again.nodes[e.a].name == "c" || again.nodes[e.b].name == "c";
        assert_eq!(
            e.fresh, has_c,
            "{} {}",
            again.nodes[e.a].name, again.nodes[e.b].name
        );
    }
    assert!(again.nodes.iter().any(|n| n.tip == c_tip));
    Ok(())
}

#[test]
fn without_a_default_branch_there_is_no_landing_order() -> Result<(), String> {
    let repo = three_branches("radar-no-target")?;
    repo.git(&["branch", "-m", "main", "trunk"])?;
    let (radar, _) = read::radar(repo.path(), Cache::default(), crate::git::now())?;
    assert!(radar.target().is_none());
    // `trunk` is active too now, as a branch like the others.
    assert_eq!(radar.branches().len(), 4);
    assert!(radar.order.is_empty());
    assert!(radar.provenance.scope.contains("no default branch"));
    assert!(read::radar(&repo.path().join("missing"), Cache::default(), 0).is_err());
    Ok(())
}
