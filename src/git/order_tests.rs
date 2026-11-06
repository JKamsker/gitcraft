//! The graph's history in each order, and first parent only, on a real repository whose
//! commits have dates of their own.

use std::path::{Path, PathBuf};

use super::{LogOrder, LogStream, Order, log, run, search};
use crate::graph::layout_with;

/// A commit with `parents`, authored at `author` and committed at `committer` (seconds), made
/// as a raw object so that its dates are exactly those.
fn commit(dir: &Path, parents: &[&str], author: i64, committer: i64, msg: &str) -> String {
    let tree = run(Some(dir), &["write-tree"]).unwrap_or_default();
    let mut text = format!("tree {}\n", tree.trim());
    for p in parents {
        text.push_str(&format!("parent {p}\n"));
    }
    text.push_str(&format!(
        "author Ünï Cödé <a@b> {author} +0000\ncommitter C <c@d> {committer} +0000\n\n{msg}\n"
    ));
    let file = dir.join(".git").join("raw commit.txt");
    let _ = std::fs::write(&file, text);
    let file = file.to_string_lossy().into_owned();
    let made = run(Some(dir), &["hash-object", "-t", "commit", "-w", &file]);
    made.unwrap_or_default().trim().to_owned()
}

/// `main`: base, m1, then x merging `side` (s1, s2). By commit date x s2 m1 s1 base; m1 was
/// written (author date) after s2.
struct History {
    dir: PathBuf,
    base: String,
    m1: String,
    s1: String,
    s2: String,
    x: String,
}

fn history(name: &str) -> History {
    let dir = std::env::temp_dir().join(format!("gitcraft-order {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    assert!(run(Some(&dir), &["init", "-q", "-b", "main"]).is_ok());
    let base = commit(&dir, &[], 100, 100, "base");
    let m1 = commit(&dir, &[&base], 450, 300, "m1 ünï");
    let s1 = commit(&dir, &[&base], 200, 200, "s1");
    let s2 = commit(&dir, &[&s1], 400, 400, "s2");
    let x = commit(&dir, &[&m1, &s2], 500, 500, "Merge side");
    for (branch, at) in [("refs/heads/main", &x), ("refs/heads/side", &s2)] {
        assert!(run(Some(&dir), &["update-ref", branch, at]).is_ok());
    }
    History {
        dir,
        base,
        m1,
        s1,
        s2,
        x,
    }
}

fn hashes(dir: &Path, order: LogOrder) -> Vec<String> {
    let commits = log(dir, None, order, 0, 100).unwrap_or_default();
    commits.into_iter().map(|c| c.hash).collect()
}

fn read(order: Order, first_parent: bool) -> LogOrder {
    LogOrder {
        order,
        first_parent,
    }
}

#[test]
fn each_order_lists_the_history_its_way() {
    let h = history("orders");
    let date = hashes(&h.dir, read(Order::Date, false));
    let author = hashes(&h.dir, read(Order::AuthorDate, false));
    let topo = hashes(&h.dir, read(Order::Topo, false));
    let _ = std::fs::remove_dir_all(&h.dir);
    assert_eq!(
        date,
        [
            h.x.as_str(),
            h.s2.as_str(),
            h.m1.as_str(),
            h.s1.as_str(),
            h.base.as_str()
        ]
    );
    assert_eq!(
        author,
        [
            h.x.as_str(),
            h.m1.as_str(),
            h.s2.as_str(),
            h.s1.as_str(),
            h.base.as_str()
        ]
    );
    // The side branch's commits together, not with m1 between them.
    let at = |hash: &str| topo.iter().position(|c| c == hash);
    assert_eq!(at(&h.s1), at(&h.s2).map(|i| i + 1), "{topo:?}");
    assert_eq!(topo.first(), Some(&h.x));
    assert_eq!(topo.last(), Some(&h.base));
}

/// First parent only: the merge, not what it brought in (its branch deleted once merged, as
/// a pull request's is; a branch still there is a tip of its own, and shows); laid out so, one
/// lane and nothing left waiting. Every parent of every commit still comes before it, in each
/// order.
#[test]
fn first_parent_leaves_out_what_merges_brought_in_and_the_graph_stays_whole() {
    let h = history("first-parent");
    let side_kept = log(&h.dir, None, read(Order::Date, true), 0, 100).unwrap_or_default();
    assert!(run(Some(&h.dir), &["update-ref", "-d", "refs/heads/side"]).is_ok());
    let mut seen = Vec::new();
    for order in Order::ALL {
        for first_parent in [false, true] {
            let commits = log(&h.dir, None, read(order, first_parent), 0, 100);
            seen.push((order, first_parent, commits.unwrap_or_default()));
        }
    }
    let streamed = LogStream::start(&h.dir, None, read(Order::Topo, true), 1)
        .and_then(|mut s| s.next_page(10));
    let found = search(&h.dir, None, read(Order::Date, true), "s1");
    let found_all = search(&h.dir, None, read(Order::Date, false), "s1");
    let _ = std::fs::remove_dir_all(&h.dir);
    for (order, first_parent, commits) in &seen {
        let listed: Vec<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
        if *first_parent {
            assert_eq!(
                listed,
                [h.x.as_str(), h.m1.as_str(), h.base.as_str()],
                "{order:?}"
            );
        } else {
            assert_eq!(listed.len(), 5, "{order:?}");
        }
        // A child is always above its parents: what the lanes are drawn from.
        for (i, c) in commits.iter().enumerate() {
            for p in c.parents.iter().take(if *first_parent { 1 } else { 2 }) {
                assert!(
                    listed[i + 1..].contains(&p.as_str()),
                    "{order:?} {listed:?}"
                );
            }
        }
        let graph = layout_with(commits, *first_parent);
        let last = graph
            .rows
            .last()
            .map(|r| r.outgoing.len() + r.through.len());
        assert_eq!(
            last,
            Some(0),
            "{order:?} {first_parent}: a lane runs past the end"
        );
        assert_eq!(graph.width, if *first_parent { 1 } else { 2 });
    }
    // The merge itself is still a merge (two parents), so it is drawn as one.
    assert_eq!(seen[1].2[0].parents.len(), 2);
    let streamed: Vec<String> = streamed
        .unwrap_or_default()
        .into_iter()
        .map(|c| c.hash)
        .collect();
    assert_eq!(streamed, [h.m1.as_str(), h.base.as_str()]);
    assert_eq!(found, Ok(Vec::new()));
    assert_eq!(side_kept.len(), 5);
    assert_eq!(found_all, Ok(vec![h.s1.clone()]));
}
