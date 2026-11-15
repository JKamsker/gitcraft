use super::*;
#[test]
fn one_index_resolves_thousands_of_distinct_decorated_commits() {
    let refs = Refs {
        local: (0..3000)
            .map(|i| crate::git::Branch {
                name: format!("feature-{i}"),
                target: format!("{i:040x}"),
                upstream: String::new(),
                track: String::new(),
                is_head: false,
            })
            .collect(),
        remotes: vec![("origin".to_owned(), Vec::new())],
        ..Default::default()
    };
    let index = BadgeIndex::new(&refs);
    for i in 0..3000 {
        let name = format!("feature-{i}");
        let badges = index.badges(&[name.clone(), format!("origin/{name}")]);
        assert_eq!(badges.len(), 2);
        assert_eq!(badges[0].label, name);
        assert!(badges[0].local);
        assert!(badges[0].remotes.is_empty());
        assert_eq!(badges[1].remotes, ["origin"]);
        assert_eq!(badges[1].full_ref, format!("refs/remotes/origin/{name}"));
    }
}

#[test]
fn full_ref_identity_survives_local_remote_and_tag_name_collisions() {
    let refs = Refs {
        local: vec![super::tests::branch("origin/topic")],
        remotes: vec![("origin".into(), vec![super::tests::branch("origin/topic")])],
        ..Default::default()
    };
    let got = BadgeIndex::new(&refs).badges(&[
        "heads/origin/topic".into(),
        "remotes/origin/topic".into(),
        "tag: topic".into(),
    ]);
    let full: std::collections::BTreeSet<_> = got.iter().map(|b| b.git_ref()).collect();
    assert_eq!(
        full,
        [
            "refs/heads/origin/topic",
            "refs/remotes/origin/topic",
            "refs/tags/topic"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
}

#[test]
fn symbolic_decorations_are_excluded_without_hiding_real_head_paths() {
    let refs = Refs {
        remotes: vec![(
            "origin".into(),
            vec![super::tests::branch("origin/nested/HEAD")],
        )],
        symbolic: ["refs/remotes/origin/default".into()].into_iter().collect(),
        ..Default::default()
    };
    let got =
        BadgeIndex::new(&refs).badges(&["origin/default".into(), "origin/nested/HEAD".into()]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].full_ref, "refs/remotes/origin/nested/HEAD");
}
