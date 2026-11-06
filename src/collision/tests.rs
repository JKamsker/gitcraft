use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::merge_tree::parse;
use super::{Matrix, Pair, compute, landing_order, merges, simulate};

/// A matrix of `n` branches where the pairs in `conflicts` conflict on one file each, and the
/// branches in `bad` conflict with the target.
fn matrix(n: usize, conflicts: &[(usize, usize)], bad: &[usize]) -> Matrix {
    let pair = |c: bool| {
        Ok(Pair {
            files: if c { vec!["f".to_owned()] } else { Vec::new() },
        })
    };
    let cells = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let c = conflicts.contains(&(i, j)) || conflicts.contains(&(j, i));
                    (i != j).then(|| pair(c))
                })
                .collect()
        })
        .collect();
    Matrix {
        branches: (0..n).map(|i| format!("b{i}")).collect(),
        target: "main".into(),
        cells,
        with_target: (0..n).map(|i| pair(bad.contains(&i))).collect(),
    }
}

fn order(m: &Matrix) -> Vec<usize> {
    landing_order(m).iter().map(|s| s.branch).collect()
}

#[test]
fn the_branch_in_everyones_way_lands_last() {
    // 0 conflicts with 1 and 2; 1 and 2 don't with each other.
    let m = matrix(3, &[(0, 1), (0, 2)], &[]);
    let steps = landing_order(&m);
    assert_eq!(order(&m), vec![1, 2, 0]);
    assert!(steps[0].clean() && steps[1].clean());
    assert_eq!(steps[2].with_earlier, vec![(1, 1), (2, 1)]);
}

#[test]
fn no_conflicts_keeps_every_branch_clean() {
    let m = matrix(4, &[], &[]);
    let steps = landing_order(&m);
    assert_eq!(steps.len(), 4);
    assert!(steps.iter().all(super::Step::clean));
}

#[test]
fn a_branch_conflicting_with_the_target_needs_rework_wherever_it_goes() {
    let m = matrix(3, &[], &[1]);
    let steps = landing_order(&m);
    assert_eq!(order(&m), vec![0, 2, 1]);
    assert_eq!(steps[2].with_target, 1);
    assert!(!steps[2].clean());
}

#[test]
fn the_largest_clean_set_beats_a_greedy_pick() {
    // A path 0-1-2-3-4 plus 5 conflicting with 0, 2 and 4: greedy by fewest conflicts can go
    // wrong; the largest set with no conflict is {1, 3, 5}... or {0, 2, 4}: three either way.
    let m = matrix(
        6,
        &[(0, 1), (1, 2), (2, 3), (3, 4), (5, 0), (5, 2), (5, 4)],
        &[],
    );
    let steps = landing_order(&m);
    let clean = steps.iter().filter(|s| s.clean()).count();
    assert_eq!(clean, 3);
    // Every branch is in the order once.
    let mut all = order(&m);
    all.sort_unstable();
    assert_eq!(all, (0..6).collect::<Vec<_>>());
}

#[test]
fn many_branches_fall_back_to_a_greedy_set() {
    let n = 24;
    let conflicts: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
    let m = matrix(n, &conflicts, &[]);
    let steps = landing_order(&m);
    assert_eq!(steps.len(), n);
    // On a path of 24, every other one: 12 land clean.
    assert_eq!(steps.iter().filter(|s| s.clean()).count(), 12);
}

#[test]
fn empty_and_single() {
    assert!(landing_order(&matrix(0, &[], &[])).is_empty());
    assert_eq!(order(&matrix(1, &[], &[])), vec![0]);
    assert_eq!(merges(0), 0);
    assert_eq!(merges(1), 1);
    assert_eq!(merges(4), 10);
}

#[test]
fn merge_tree_output() {
    assert!(parse("5e48142ef3f9c983f76c9e44581c4da7a1ba59c6\0").clean());
    assert!(parse("5e48142ef3f9c983f76c9e44581c4da7a1ba59c6\n").clean());
    let pair = parse("5e48\0f\0sp ace.txt\0f\0\0messages");
    assert_eq!(pair.files, vec!["f", "sp ace.txt"]);
    assert!(parse("").clean());
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-collision-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(&dir, args)?;
    }
    Ok(dir)
}

/// On `branch` (made from `from`), `file` set to `text`, committed.
fn change(dir: &Path, from: &str, branch: &str, file: &str, text: &str) -> Result<(), String> {
    git(dir, &["checkout", "-q", "-B", branch, from])?;
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", branch])?;
    Ok(())
}

#[test]
fn a_real_repository_with_agent_branches() -> Result<(), String> {
    let dir = repo("real")?;
    std::fs::write(dir.join("shared file.txt"), "a\nb\nc\n").map_err(|e| e.to_string())?;
    std::fs::write(dir.join("ünï.txt"), "1\n").map_err(|e| e.to_string())?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "base"])?;
    // Two agents rewrite the same line; a third works elsewhere; main moved on meanwhile.
    change(&dir, "main", "agent/one", "shared file.txt", "a\nONE\nc\n")?;
    change(&dir, "main", "agent/two", "shared file.txt", "a\nTWO\nc\n")?;
    change(&dir, "main", "agent/ünï", "new.txt", "new\n")?;
    change(&dir, "main", "agent/stale", "ünï.txt", "stale\n")?;
    change(&dir, "main", "main", "ünï.txt", "main\n")?;
    let head_before = git(&dir, &["rev-parse", "HEAD"])?;

    let pair = simulate(&dir, "agent/one", "agent/two")?;
    assert_eq!(pair.files, vec!["shared file.txt"]);
    assert!(simulate(&dir, "agent/one", "agent/ünï")?.clean());

    let branches: Vec<String> = ["agent/one", "agent/two", "agent/ünï", "agent/stale"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let done = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicUsize::new(0));
    let m = compute(&dir, &branches, "main", &done, &cancel)?;
    assert_eq!(done.load(Ordering::Relaxed), merges(4));
    assert!(m.conflict(0, 1) && m.conflict(1, 0));
    assert!(!m.conflict(0, 2));
    assert_eq!(m.target_count(3), 1);
    assert!(m.first_error().is_none());
    let steps = landing_order(&m);
    let names: Vec<&str> = steps
        .iter()
        .map(|s| m.branches[s.branch].as_str())
        .collect();
    // One of the two rewriting the same line, the one elsewhere, then the other and the
    // stale one (conflicting with main).
    assert_eq!(steps.iter().filter(|s| s.clean()).count(), 2, "{names:?}");
    let mut last_two = names[2..].to_vec();
    last_two.sort_unstable();
    assert_eq!(last_two, vec!["agent/stale", "agent/two"], "{names:?}");
    let stale = steps.iter().find(|s| m.branches[s.branch] == "agent/stale");
    assert_eq!(stale.map(|s| s.with_target), Some(1));
    // Nothing checked out or written to the working tree.
    assert_eq!(git(&dir, &["rev-parse", "HEAD"])?, head_before);
    assert!(git(&dir, &["status", "--porcelain"])?.is_empty());

    cancel.store(1, Ordering::Relaxed);
    assert!(compute(&dir, &branches, "main", &done, &cancel).is_err());
    let missing = simulate(&dir, "agent/one", "no-such-branch");
    assert!(missing.is_err_and(|e| e.starts_with("Could not merge agent/one with no-such-branch")));
    assert!(simulate(&dir, "--output=x", "main").is_err());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
