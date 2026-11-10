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
        assert_eq!(badges.len(), 1);
        assert_eq!(badges[0].label, name);
        assert!(badges[0].local);
        assert_eq!(badges[0].remotes, ["origin"]);
    }
}
