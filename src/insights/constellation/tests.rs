use std::collections::{BTreeMap, HashMap, HashSet};

use super::read;
use super::*;
use crate::git::fixture::Repo;

fn branch(name: &str, tip: &str, upstream: &str, ahead: u32) -> BranchIn {
    BranchIn {
        name: name.to_owned(),
        tip: tip.to_owned(),
        upstream: upstream.to_owned(),
        ahead,
        ..BranchIn::default()
    }
}

fn worktree(path: &str, branch: &str, changes: Option<usize>) -> WorktreeIn {
    WorktreeIn {
        path: path.to_owned(),
        branch: Some(branch.to_owned()),
        head: "h".to_owned(),
        changes,
        ..WorktreeIn::default()
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        branches: vec![
            branch("main", "m1", "origin/main", 0),
            branch("feat", "f1", "", 0),
            branch("fix", "x1", "origin/fix", 0),
            branch("old", "o1", "origin/old", 0),
        ],
        worktrees: vec![
            worktree("/w/main", "main", Some(0)),
            worktree("/w/feat", "feat", Some(3)),
        ],
        tasks: vec![TaskIn {
            branch: "feat".to_owned(),
            path: "/w/feat".to_owned(),
            agent: "Codex".to_owned(),
            turns: 2,
            last_turn: Some(1_000),
            ..TaskIn::default()
        }],
        target: Some(Target {
            name: "main".to_owned(),
            rev: "refs/heads/main".to_owned(),
            tip: "m1".to_owned(),
        }),
        merged: Some(HashSet::from(["main".to_owned(), "old".to_owned()])),
        bases: HashMap::from([
            ("feat".to_owned(), "b0".to_owned()),
            ("fix".to_owned(), "b0".to_owned()),
        ]),
        ..Snapshot::default()
    }
}

/// The flags of the node of `kind` keyed so (`[Missing, Missing]` when there is none, which
/// no node has).
fn flags(m: &Model, kind: Kind, key: &str) -> Vec<Flag> {
    match m.find(kind, key) {
        Some(i) => m.nodes[i].flags.iter().map(|(f, _)| *f).collect(),
        None => vec![Flag::Missing, Flag::Missing],
    }
}

fn linked(m: &Model, a: (Kind, &str), b: (Kind, &str), link: Link) -> bool {
    let (Some(a), Some(b)) = (m.find(a.0, a.1), m.find(b.0, b.1)) else {
        return false;
    };
    m.edges
        .iter()
        .any(|e| e.link == link && ((e.from == a && e.to == b) || (e.from == b && e.to == a)))
}

#[test]
fn states_come_from_what_git_and_the_forge_say() {
    let forge = Forge {
        prs: Some(vec![PrIn {
            number: 7,
            branch: "fix".to_owned(),
            sha: "x1".to_owned(),
            ..PrIn::default()
        }]),
        builds: Some(HashMap::from([("x1".to_owned(), CheckState::Failed)])),
    };
    let m = build(&snapshot(), &forge, 1_100);
    assert_eq!(flags(&m, Kind::Branch, "feat"), vec![Flag::Unpublished]);
    assert_eq!(flags(&m, Kind::Branch, "old"), vec![Flag::Merged]);
    // The default branch is never merged into itself.
    assert!(flags(&m, Kind::Branch, "main").is_empty());
    assert_eq!(flags(&m, Kind::Branch, "fix"), vec![Flag::Failed]);
    assert_eq!(flags(&m, Kind::Worktree, "/w/feat"), vec![Flag::Dirty]);
    assert_eq!(flags(&m, Kind::Worktree, "/w/main"), vec![Flag::Idle]);
    // Active is inferred from the last checkpoint turn, never observed.
    let task = m.find(Kind::Task, "/w/feat");
    assert_eq!(
        task.map(|i| m.nodes[i].flags.clone()),
        Some(vec![(Flag::Active, Evidence::Inferred)])
    );
    let later = build(&snapshot(), &forge, 1_000 + ACTIVE_WITHIN + 1);
    assert_eq!(flags(&later, Kind::Task, "/w/feat"), vec![Flag::Idle]);
    assert_eq!(flags(&m, Kind::Build, "x1"), vec![Flag::Failed]);
}

#[test]
fn edges_join_branches_worktrees_tasks_pull_requests_and_builds() {
    let mut s = snapshot();
    s.stack_parents = BTreeMap::from([("fix".to_owned(), "feat".to_owned())]);
    let forge = Forge {
        prs: Some(vec![
            PrIn {
                number: 7,
                branch: "fix".to_owned(),
                sha: "x1".to_owned(),
                ..PrIn::default()
            },
            // A fork's branch of the same name is not this one.
            PrIn {
                number: 8,
                branch: "feat".to_owned(),
                sha: "zz".to_owned(),
                from_fork: true,
                ..PrIn::default()
            },
        ]),
        builds: Some(HashMap::from([("x1".to_owned(), CheckState::Passed)])),
    };
    let m = build(&s, &forge, 0);
    let b = |k| (Kind::Branch, k);
    assert!(linked(
        &m,
        b("feat"),
        (Kind::Worktree, "/w/feat"),
        Link::CheckedOut
    ));
    assert!(linked(
        &m,
        (Kind::Worktree, "/w/feat"),
        (Kind::Task, "/w/feat"),
        Link::TaskWorktree
    ));
    assert!(linked(
        &m,
        b("feat"),
        (Kind::Task, "/w/feat"),
        Link::TaskBranch
    ));
    assert!(linked(&m, b("fix"), (Kind::Pr, "7"), Link::Proposes));
    assert!(m.find(Kind::Pr, "8").is_none());
    // The pull request's head is the tip: one build node, built for both.
    assert!(linked(&m, b("fix"), (Kind::Build, "x1"), Link::Built));
    assert!(linked(
        &m,
        (Kind::Pr, "7"),
        (Kind::Build, "x1"),
        Link::Built
    ));
    assert_eq!(m.nodes.iter().filter(|n| n.kind == Kind::Build).count(), 1);
    // A recorded parent wins over the inferred same base; merged branches have none.
    assert!(linked(&m, b("fix"), b("feat"), Link::StackParent));
    assert!(!m.edges.iter().any(|e| e.link == Link::SameBase));
    let m = build(&snapshot(), &forge, 0);
    let same = m.edges.iter().find(|e| e.link == Link::SameBase);
    assert_eq!(same.map(|e| e.evidence), Some(Evidence::Inferred));
}

#[test]
fn without_forge_data_nothing_is_made_up_and_it_says_so() {
    let m = build(&snapshot(), &Forge::default(), 0);
    assert!(
        m.nodes
            .iter()
            .all(|n| !matches!(n.kind, Kind::Pr | Kind::Build))
    );
    assert!(
        m.notes
            .iter()
            .any(|n| n.starts_with("Pull requests: not read"))
    );
    assert!(m.notes.iter().any(|n| n.starts_with("Builds: not read")));
    // No build state is shown as failing or passing.
    assert!(m.nodes.iter().all(|n| {
        n.flags
            .iter()
            .all(|(f, _)| !matches!(f, Flag::Failed | Flag::Passed | Flag::Running))
    }));
    // Without a default branch, merged isn't claimed either way.
    let mut s = snapshot();
    s.target = None;
    s.merged = None;
    let m = build(&s, &Forge::default(), 0);
    assert!(!flags(&m, Kind::Branch, "old").contains(&Flag::Merged));
    assert!(m.notes.iter().any(|n| n.starts_with("No default branch")));
}

#[test]
fn a_task_without_turns_has_no_activity_and_a_gone_worktree_is_missing() {
    let mut s = snapshot();
    s.tasks[0].last_turn = None;
    s.tasks.push(TaskIn {
        branch: "gone".to_owned(),
        path: "/w/gone".to_owned(),
        agent: "Aider".to_owned(),
        ..TaskIn::default()
    });
    let m = build(&s, &Forge::default(), 0);
    assert!(flags(&m, Kind::Task, "/w/feat").is_empty());
    assert_eq!(flags(&m, Kind::Task, "/w/gone"), vec![Flag::Missing]);
    // No branch node is invented for a task's branch that isn't there.
    assert!(m.find(Kind::Branch, "gone").is_none());
}

#[test]
fn branches_are_bounded_keeping_those_in_use() {
    let all: Vec<BranchIn> = (0..MAX_BRANCHES + 5)
        .map(|i| branch(&format!("b{i}"), "t", "", 0))
        .collect();
    let keep = HashSet::from([format!("b{}", MAX_BRANCHES + 4)]);
    let (kept, cut) = read::bound_branches(all, &keep);
    assert!(cut);
    assert_eq!(kept.len(), MAX_BRANCHES);
    assert_eq!(kept[0].name, format!("b{}", MAX_BRANCHES + 4));
}

#[test]
fn moved_tips_are_told() {
    let tips = vec![
        ("a".to_owned(), "1".to_owned()),
        ("b".to_owned(), "2".to_owned()),
    ];
    assert!(moved(&tips, |n| Some(if n == "a" { "1" } else { "2" }.to_owned())).is_empty());
    assert_eq!(
        moved(&tips, |n| (n == "a").then(|| "1".to_owned())),
        vec!["b"]
    );
}

#[test]
fn branches_are_parsed_with_their_upstream_state() {
    let out = "refs/heads/feat/x\u{1f}abc\u{1f}origin/feat/x\u{1f}[ahead 2, behind 1]\u{1f}17\n\
               refs/heads/old\u{1f}def\u{1f}origin/old\u{1f}[gone]\u{1f}9\n";
    let list = read::parse_branches(out);
    assert_eq!(list[0].name, "feat/x");
    assert_eq!(
        (list[0].ahead, list[0].behind, list[0].committed),
        (2, 1, 17)
    );
    assert!(list[1].gone);
    assert_eq!(read::human_bytes(999), "999 B");
    assert_eq!(read::human_bytes(12_300_000), "12.3 MB");
}

#[test]
fn a_real_worktree_changes_state_after_a_commit() -> Result<(), String> {
    let repo = Repo::new("constellation")?;
    repo.commit("base", &[("a.txt", Some("a\n"))])?;
    repo.git(&["branch", "old"])?;
    repo.git(&["branch", "feat"])?;
    let wt = repo.path().join("wt feat");
    let wt_str = wt.to_string_lossy().into_owned();
    repo.git(&["worktree", "add", "-q", &wt_str, "feat"])?;
    std::fs::write(wt.join("b.txt"), "b\n").map_err(|e| e.to_string())?;

    let first = read::snapshot(repo.path(), crate::git::now())?;
    let m = build(&first, &Forge::default(), 0);
    let wt_key = first
        .worktrees
        .iter()
        .find(|w| w.branch.as_deref() == Some("feat"))
        .map(|w| w.path.clone())
        .ok_or("no worktree")?;
    assert_eq!(flags(&m, Kind::Worktree, &wt_key), vec![Flag::Dirty]);
    assert!(linked(
        &m,
        (Kind::Branch, "feat"),
        (Kind::Worktree, &wt_key),
        Link::CheckedOut
    ));
    // At the default branch's tip, both are merged; no upstream: unpublished.
    assert_eq!(first.target.as_ref().map(|t| t.name.as_str()), Some("main"));
    assert!(flags(&m, Kind::Branch, "feat").contains(&Flag::Merged));
    assert!(flags(&m, Kind::Branch, "old").contains(&Flag::Unpublished));
    assert!(first.provenance.partial.is_none());

    // A commit in the worktree: clean, no longer merged, its tip moved.
    crate::git::run(Some(&wt), &["add", "-A"])?;
    crate::git::run(Some(&wt), &["commit", "-q", "-m", "feat"])?;
    let second = read::snapshot(repo.path(), crate::git::now())?;
    let m2 = build(&second, &Forge::default(), 0);
    assert_eq!(flags(&m2, Kind::Worktree, &wt_key), vec![Flag::Idle]);
    assert!(!flags(&m2, Kind::Branch, "feat").contains(&Flag::Merged));
    let tip = |name: &str| {
        second
            .branches
            .iter()
            .find(|b| b.name == name)
            .map(|b| b.tip.clone())
    };
    assert_eq!(moved(&first.tips, tip), vec!["feat"]);
    // Forked where `old` is: the same base as nothing else unmerged, so no inferred edge.
    assert!(!m2.edges.iter().any(|e| e.link == Link::SameBase));

    // Its folder, its `.git` file aside.
    let disk = read::disk_usage(&wt, read::DISK_ENTRIES)?;
    assert_eq!(disk.files, 2);
    assert!(!disk.partial);
    let cut = read::disk_usage(&wt, 1)?;
    assert!(cut.partial);
    assert!(read::disk_usage(&repo.path().join("nope"), 10).is_err());
    Ok(())
}
