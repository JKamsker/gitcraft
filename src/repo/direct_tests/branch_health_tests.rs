use super::*;

fn branch(name: &str, track: &str) -> Branch {
    Branch {
        name: name.to_owned(),
        target: "a".repeat(40),
        is_head: false,
        upstream: String::new(),
        track: track.to_owned(),
    }
}

fn pr(branch: &str, base: &str) -> MergedPullRequest {
    MergedPullRequest {
        number: 3,
        branch: branch.to_owned(),
        sha: "s".to_owned(),
        base: base.to_owned(),
    }
}

#[test]
fn reasons_combine_merged_gone_and_stale() {
    let now = 1_000 * DAY;
    let tip = "a".repeat(40);
    let health = Health {
        by_date: vec!["fresh".to_owned(), "old".to_owned()],
        dates: [
            ("fresh".to_owned(), now - DAY),
            ("old".to_owned(), now - 120 * DAY),
        ]
        .into_iter()
        .collect(),
        merged: [("old".to_owned(), (tip, Merged::Pr(7, "main".to_owned())))]
            .into_iter()
            .collect(),
        default: "main".to_owned(),
    };
    let pr7 = Reason::Merged(Merged::Pr(7, "main".to_owned()));
    assert!(health.reasons(&branch("fresh", ""), now, 90).is_empty());
    assert_eq!(
        health.reasons(&branch("old", "[gone]"), now, 90),
        vec![pr7.clone(), Reason::UpstreamGone, Reason::Stale(120)]
    );
    // 0 days: nothing is ever stale.
    assert_eq!(health.reasons(&branch("old", ""), now, 0), vec![pr7]);
    // A branch the read didn't see (made since) has no reasons yet.
    assert!(health.reasons(&branch("new", ""), now, 1).is_empty());
    // A commit on it since the read: not known to be merged any more.
    let mut moved = branch("old", "");
    moved.target = "b".repeat(40);
    assert!(health.reasons(&moved, now, 0).is_empty());
    // The default branch is never stale, however long since its last commit.
    let mut health = health;
    health.dates.insert("main".to_owned(), 0);
    assert!(health.reasons(&branch("main", ""), now, 90).is_empty());
    // Another repository's pull requests say nothing here.
    health.forget_prs();
    assert!(health.merged.is_empty());
}

#[test]
fn the_key_follows_what_was_read_from() {
    let bases = Bases {
        name: "main".to_owned(),
        refs: vec![("refs/heads/main".to_owned(), "1".repeat(40))],
    };
    let a = key_of(&[branch("x", "")], &bases, &[]);
    let b = key_of(&[branch("y", "")], &bases, &[]);
    assert_eq!(a, key_of(&[branch("x", "")], &bases, &[]));
    assert_ne!(a, b);
    assert_ne!(a, key_of(&[branch("x", "")], &bases, &[pr("x", "main")]));
}

/// One read at a time; a change while it runs is read once it is back; a failed read is
/// asked again rather than never.
#[test]
fn reads_run_one_at_a_time_and_failures_are_retried() {
    let mut state = HealthState::default();
    assert!(state.checking());
    assert!(state.should_read(1));
    assert!(!state.should_read(1), "the same read is asked for once");
    assert!(!state.should_read(2), "not while one runs");
    assert!(state.arrived(Ok((Health::default(), CherryCache::new()))));
    assert!(state.health.is_some() && !state.checking());
    assert!(state.should_read(2));
    assert!(!state.arrived(Err("boom".to_owned())));
    assert!(state.error.as_deref().is_some_and(|e| e.contains("boom")));
    assert!(state.should_read(2), "a failed read is tried again");
}

/// Only pull requests into the default branch count: one merged into another branch (a
/// stacked pull request) says nothing about the default branch.
#[test]
fn pull_requests_into_other_branches_do_not_count() {
    let bases = Bases {
        name: "main".to_owned(),
        refs: Vec::new(),
    };
    let inputs = Inputs::new(
        &[branch("part1", "")],
        bases,
        &[pr("part1", "feature-big"), pr("part2", "main")],
    );
    let branches: Vec<&str> = inputs
        .merged_prs
        .iter()
        .map(|p| p.branch.as_str())
        .collect();
    assert_eq!(branches, ["part2"]);
}

#[test]
fn short_names_of_bases() {
    assert_eq!(short_ref("refs/heads/main"), "main");
    assert_eq!(short_ref("refs/remotes/my/fork/main"), "my/fork/main");
    assert_eq!(short_ref("HEAD"), "HEAD");
}
