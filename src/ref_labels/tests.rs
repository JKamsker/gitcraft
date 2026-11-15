use super::*;

fn remote(alias: &str, path: &str, owner: Option<&str>) -> Entry {
    Entry {
        full_ref: format!("refs/remotes/{alias}/{path}"),
        kind: Kind::Remote,
        path: path.into(),
        remote: Some(alias.into()),
        owner: owner.map(str::to_owned),
    }
}

fn caption<'a>(labels: &'a BTreeMap<String, Label>, alias: &str, path: &str) -> &'a str {
    &labels[&format!("refs/remotes/{alias}/{path}")].caption
}

#[test]
fn compact_preferred_and_missing_preferred() {
    let entries = [
        remote("origin", "topic", None),
        remote("fork", "topic", Some("alice")),
    ];
    let got = labels(&entries, None);
    assert_eq!(caption(&got, "origin", "topic"), "topic");
    assert_eq!(caption(&got, "fork", "topic"), "alice/topic");
    let entries = [
        remote("origin", "main", None),
        remote("a", "topic", Some("alice")),
        remote("b", "topic", Some("bob")),
    ];
    let got = labels(&entries, None);
    assert_eq!(caption(&got, "a", "topic"), "alice/topic");
    assert_eq!(caption(&got, "b", "topic"), "bob/topic");
}

#[test]
fn ambiguous_owners_use_aliases_regardless_of_url_host_or_repository() {
    for preferred in [None, Some("third")] {
        let entries = [
            remote("one", "topic", Some("alice")),
            remote("two", "topic", Some("alice")),
            remote("third", "main", None),
        ];
        let got = labels(&entries, preferred);
        assert_eq!(caption(&got, "two", "topic"), "two/topic");
    }
}

#[test]
fn reserve_literal_paths_and_full_ref_fallbacks() {
    let entries = [
        remote("origin", "topic", None),
        remote("fork", "topic", Some("alice")),
        remote("other", "alice/topic", None),
        remote("last", "fork/topic", None),
    ];
    let got = labels(&entries, None);
    assert_eq!(caption(&got, "fork", "topic"), "refs/remotes/fork/topic");
    assert_eq!(
        got.values()
            .map(|l| &l.caption)
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
}

#[test]
fn local_and_tag_captions_are_reserved_before_remote_qualification() {
    let mut entries = vec![remote("my/fork", "日本語", None)];
    for (prefix, kind) in [("heads", Kind::Local), ("tags", Kind::Tag)] {
        entries.push(Entry {
            full_ref: format!("refs/{prefix}/日本語"),
            path: "日本語".into(),
            kind,
            remote: None,
            owner: None,
        });
    }
    let got = labels(&entries, None);
    assert_eq!(got["refs/heads/日本語"].caption, "日本語");
    assert_eq!(got["refs/tags/日本語"].caption, "refs/tags/日本語");
    assert_eq!(caption(&got, "my/fork", "日本語"), "my/fork/日本語");
}

#[test]
fn preference_and_order_do_not_change_identity() {
    let mut entries = vec![
        remote("origin", "topic", None),
        remote("alice", "topic", Some("alice")),
    ];
    let a = labels(&entries, Some("alice"));
    entries.reverse();
    assert_eq!(a, labels(&entries, Some("alice")));
    assert_eq!(caption(&a, "alice", "topic"), "topic");
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        labels(&entries, None).keys().collect::<Vec<_>>()
    );
}
