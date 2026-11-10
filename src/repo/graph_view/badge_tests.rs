use super::*;
use crate::git;

fn refs_with_origin() -> Refs {
    Refs {
        remotes: vec![("origin".to_owned(), Vec::new())],
        ..Refs::default()
    }
}

#[test]
fn remote_twin_folds_into_local_badge() {
    let decos = [
        "origin/main",
        "HEAD -> main",
        "origin/HEAD",
        "origin/dev",
        "tag: v1",
    ]
    .map(String::from);
    let b = badges(&decos, &refs_with_origin());
    let labels: Vec<&str> = b.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, vec!["main", "dev", "v1"]);
    assert!(b[0].head && b[0].local && b[0].remotes == ["origin"]);
    assert!(!b[1].local && b[1].remotes == ["origin"]);
    assert_eq!(b[2].kind, BadgeKind::Tag);
}

/// The refs gitcraft keeps earlier versions in make no pill, however git lists them.
#[test]
fn kept_versions_are_no_branches() {
    let decos = [
        "HEAD -> main",
        "refs/gitcraft/rewrites/main/12/new-tip",
        "refs/gitcraft/reviewed/github.com/a/b/3/head",
    ]
    .map(String::from);
    let labels: Vec<String> = badges(&decos, &refs_with_origin())
        .into_iter()
        .map(|b| b.label)
        .collect();
    assert_eq!(labels, ["main"]);
}

fn branch(name: &str) -> git::Branch {
    git::Branch {
        name: name.to_owned(),
        target: String::new(),
        is_head: false,
        upstream: String::new(),
        track: String::new(),
    }
}

#[test]
fn a_remote_named_like_the_start_of_another_does_not_take_its_branches() {
    let refs = Refs {
        remotes: vec![
            ("my".to_owned(), vec![branch("my/dev")]),
            ("my/fork".to_owned(), vec![branch("my/fork/main")]),
        ],
        ..Refs::default()
    };
    let b = badges(&["my/fork/main".to_owned()], &refs);
    assert_eq!(b[0].label, "main");
    assert_eq!(b[0].remotes, ["my/fork"]);
    assert_eq!(b[0].git_ref(), "my/fork/main");
    // Not in the lists yet (refs not reloaded): still the longest remote.
    let b = badges(&["my/fork/topic".to_owned()], &refs);
    assert_eq!(
        (b[0].label.as_str(), b[0].remotes.as_slice()),
        ("topic", &["my/fork".to_owned()][..])
    );
}

#[test]
fn a_local_branch_named_under_a_remote_is_local() {
    let refs = Refs {
        local: vec![branch("origin/wip")],
        remotes: vec![("origin".to_owned(), vec![branch("origin/main")])],
        ..Refs::default()
    };
    let b = badges(&["origin/wip".to_owned()], &refs);
    assert_eq!(b[0].label, "origin/wip");
    assert!(b[0].local && b[0].remotes.is_empty());
    assert_eq!(b[0].git_ref(), "origin/wip");
}

#[test]
fn full_ref_names_make_the_same_pills() {
    // `%D` with `log.decorate=full` in the user's config.
    let refs = Refs {
        local: vec![branch("main"), branch("x")],
        remotes: vec![("origin".to_owned(), vec![branch("origin/main")])],
        ..Refs::default()
    };
    let decos = [
        "HEAD -> refs/heads/main",
        "refs/remotes/origin/main",
        "refs/remotes/origin/HEAD",
        "tag: refs/tags/x",
        "refs/heads/x",
        "refs/remotes/gone/topic",
    ]
    .map(String::from);
    let b = badges(&decos, &refs);
    let pills: Vec<(&str, bool, &[String])> = b
        .iter()
        .map(|b| (b.label.as_str(), b.local, b.remotes.as_slice()))
        .collect();
    assert_eq!(
        pills,
        vec![
            ("main", true, &["origin".to_owned()][..]),
            ("x", true, &[][..]),
            ("topic", false, &["gone".to_owned()][..]),
            ("x", false, &[][..]),
        ]
    );
    assert!(b[0].head);
    assert_eq!(b[1].git_ref(), "x");
    assert_eq!(b[2].git_ref(), "gone/topic");
    assert_eq!(b[3].kind, BadgeKind::Tag);
}

#[test]
fn checked_out_branch_wins_even_without_a_head_decoration() {
    let refs = Refs {
        current: Some("topic".to_owned()),
        local: vec![branch("main"), branch("topic")],
        ..refs_with_origin()
    };
    let b = badges(&["main", "origin/main", "topic"].map(String::from), &refs);
    assert_eq!(b[0].label, "topic");
    assert!(b[0].head);
    assert_eq!(b[1].label, "main");
    // Live checkout metadata also takes precedence over decorations read earlier.
    let b = badges(&["HEAD -> main", "topic"].map(String::from), &refs);
    assert_eq!(b[0].label, "topic");
    assert!(!b[1].head);
}

#[test]
fn default_branch_names_win_over_other_branches_at_the_same_commit() {
    for (names, first) in [
        (vec!["topic", "dev", "master", "main"], "main"),
        (vec!["topic", "dev", "master"], "master"),
        (vec!["topic", "dev", "develop"], "dev"),
        (vec!["topic", "develop"], "develop"),
    ] {
        let decos: Vec<String> = names.into_iter().map(str::to_owned).collect();
        assert_eq!(badges(&decos, &refs_with_origin())[0].label, first);
    }
    // A default branch on the remote still represents this commit ahead of a local
    // feature branch; it needn't have been checked out locally.
    let decos = ["topic", "origin/dev"].map(String::from);
    let b = badges(&decos, &refs_with_origin());
    assert_eq!(b[0].label, "dev");
    assert!(!b[0].local);
}

#[test]
fn branch_ties_are_independent_of_git_decoration_order() {
    let refs = Refs {
        remotes: vec![("z".to_owned(), vec![]), ("a".to_owned(), vec![])],
        ..Refs::default()
    };
    let names = ["beta", "a/topic", "alpha", "z/topic", "tag: release"];
    let forward = badges(&names.map(String::from), &refs);
    let backward = badges(
        &names
            .into_iter()
            .rev()
            .map(String::from)
            .collect::<Vec<_>>(),
        &refs,
    );
    let describe =
        |badges: Vec<Badge>| badges.into_iter().map(|b| b.describe()).collect::<Vec<_>>();
    assert_eq!(describe(forward), describe(backward));
}

#[test]
fn local_branch_with_slash_is_not_remote() {
    let decos = ["feature/x".to_owned()];
    let b = badges(&decos, &refs_with_origin());
    assert!(b[0].kind == BadgeKind::Branch && b[0].local && b[0].remotes.is_empty());
}
#[test]
fn thousands_of_local_and_remote_twins_keep_head_order_and_separate_tags() {
    let names: Vec<String> = (0..3000).map(|i| format!("team/feature-{i}")).collect();
    let refs = Refs {
        local: names.iter().map(|name| branch(name)).collect(),
        remotes: vec![(
            "origin".to_owned(),
            names
                .iter()
                .map(|name| branch(&format!("origin/{name}")))
                .collect(),
        )],
        ..Refs::default()
    };
    let mut decorations: Vec<String> = names
        .iter()
        .flat_map(|name| [format!("origin/{name}"), name.clone()])
        .collect();
    decorations.push("HEAD -> team/feature-2999".to_owned());
    decorations.push("tag: team/feature-2999".to_owned());
    let got = badges(&decorations, &refs);
    assert_eq!(got.len(), 3001);
    assert_eq!(got[0].label, "team/feature-2999");
    assert!(got[0].head);
    let mut sorted = names[..2999].to_vec();
    sorted.sort();
    for (badge, name) in got[1..3000].iter().zip(&sorted) {
        assert_eq!(&badge.label, name);
        assert!(!badge.head);
    }
    assert!(
        got[..3000]
            .iter()
            .all(|b| b.kind == BadgeKind::Branch && b.local && b.remotes == ["origin"])
    );
    assert_eq!(got[3000].kind, BadgeKind::Tag);
    assert_eq!(got[3000].label, "team/feature-2999");
}
