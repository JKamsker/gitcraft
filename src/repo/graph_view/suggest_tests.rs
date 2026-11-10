use super::*;

fn person(name: &str, email: &str, commits: usize) -> Person {
    Person {
        name: name.to_owned(),
        email: email.to_owned(),
        commits,
    }
}

fn state() -> SuggestState {
    SuggestState {
        authors: Source::Ready(Arc::new(vec![
            person("Alice Smith", "alice@example.com", 30),
            person("Zoë Ünal", "zoe@example.com", 4),
            person("Bob", "bob@alice.dev", 2),
        ])),
        paths: Source::Ready(Arc::new(PathIndex::new(vec![
            "README.md".to_owned(),
            "src/main.rs".to_owned(),
            "src/git/mod.rs".to_owned(),
            "docs/Read me.md".to_owned(),
        ]))),
        ..SuggestState::default()
    }
}

fn labels(l: &Listing) -> Vec<&str> {
    l.items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn a_word_suggests_the_operators_it_starts() {
    let s = state();
    assert_eq!(labels(&listing("au", 2, &s)), ["author:"]);
    assert_eq!(labels(&listing("fix me", 6, &s)), ["message:", "merges:"]);
    assert_eq!(labels(&listing("me", 2, &s)), ["message:", "merges:"]);
    assert_eq!(labels(&listing("-me", 3, &s)), ["-merges:"]);
    assert_eq!(labels(&listing("COMM", 4, &s)), ["committer:"]);
    // One letter is most searches: nothing.
    assert!(listing("a", 1, &s).items.is_empty());
    // In the whitespace between pieces: nothing.
    assert!(listing("au  x", 3, &s).items.is_empty());
}

#[test]
fn authors_are_suggested_by_name_or_email() {
    let s = state();
    let l = listing("fix author:", 11, &s);
    assert_eq!(labels(&l), ["Alice Smith", "Zoë Ünal", "Bob"]);
    assert_eq!(l.range, 4..11);
    // Starting with it comes first, then containing it (Bob's email has “alice”).
    assert_eq!(
        labels(&listing("author:ali", 10, &s)),
        ["Alice Smith", "Bob"]
    );
    assert_eq!(labels(&listing("author:ZOË", 10, &s)), ["Zoë Ünal"]);
    let l = listing("author:zo", 9, &s);
    assert_eq!(l.items[0].insert, "author:\"Zoë Ünal\"");
    assert_eq!(l.items[0].detail, "zoe@example.com · 4 commits");
    let l = listing("author:\"Zoë", 11, &s);
    assert_eq!(labels(&l), ["Zoë Ünal"]);
    let l = listing("author:nobody", 13, &s);
    assert_eq!(l.status.as_deref(), Some("No authors match"));
}

#[test]
fn sources_are_read_when_first_needed() {
    let s = SuggestState::default();
    let l = listing("committer:", 10, &s);
    assert_eq!(l.load, Some(SourceKind::Committers));
    assert_eq!(l.status.as_deref(), Some("Reading committers…"));
    let s = SuggestState {
        paths: Source::Failed("not a repository".to_owned()),
        ..SuggestState::default()
    };
    let l = listing("path:x", 6, &s);
    assert_eq!(l.load, None);
    assert_eq!(
        l.status.as_deref(),
        Some("Couldn't read the files: not a repository")
    );
}

#[test]
fn paths_and_their_folders_are_suggested() {
    let s = state();
    assert_eq!(
        labels(&listing("path:src", 8, &s)),
        ["src/", "src/git/", "src/git/mod.rs", "src/main.rs"]
    );
    // By file name too, after the paths that start with it.
    assert_eq!(labels(&listing("path:main", 9, &s)), ["src/main.rs"]);
    assert_eq!(
        labels(&listing("path:read", 9, &s)),
        ["README.md", "docs/Read me.md"]
    );
    let l = listing("path:docs/r", 11, &s);
    assert_eq!(l.items[0].insert, "path:\"docs/Read me.md\"");
}

#[test]
fn dates_are_suggested() {
    let s = state();
    let l = listing("after:", 6, &s);
    assert_eq!(l.items.len(), MAX_SHOWN);
    assert_eq!(l.items[0].insert, "after:today");
    assert_eq!(labels(&listing("before:y", 8, &s)), ["yesterday"]);
    assert_eq!(labels(&listing("before:1", 8, &s)), ["1w", "1m", "1y"]);
    // Nothing for operators whose values can't be listed.
    assert!(listing("message:x", 9, &s).items.is_empty());
    assert!(listing("-merges:", 8, &s).items.is_empty());
}

#[test]
fn the_only_suggestion_already_typed_is_not_offered() {
    let s = state();
    assert!(listing("after:yesterday", 15, &s).items.is_empty());
    assert!(listing("author:Bob", 10, &s).items.is_empty());
}

#[test]
fn accepting_replaces_the_piece_at_the_cursor() {
    let item = |insert: &str, operator| Item {
        label: String::new(),
        detail: String::new(),
        insert: insert.to_owned(),
        operator,
    };
    assert_eq!(
        accept("fix au", &(4..6), &item("author:", true)),
        ("fix author:".to_owned(), 11)
    );
    assert_eq!(
        accept(
            "author:zo fix",
            &(0..9),
            &item("author:\"Zoë Ünal\"", false)
        ),
        ("author:\"Zoë Ünal\" fix".to_owned(), 18)
    );
    assert_eq!(
        accept("é path:sr", &(3..10), &item("path:src/", false)),
        ("é path:src/ ".to_owned(), 12)
    );
}

#[test]
fn people_without_a_name_of_their_own_are_put_in_by_email() {
    let s = SuggestState {
        authors: Source::Ready(Arc::new(vec![
            person("Sam", "sam@a.org", 3),
            person("Sam", "sam@b.org", 2),
            person("", "ghost@c.org", 1),
        ])),
        ..SuggestState::default()
    };
    let l = listing("author:", 7, &s);
    let inserts: Vec<&str> = l.items.iter().map(|i| i.insert.as_str()).collect();
    assert_eq!(
        inserts,
        [
            "author:<sam@a.org>",
            "author:<sam@b.org>",
            "author:<ghost@c.org>"
        ]
    );
    assert_eq!(labels(&l), ["Sam", "Sam", "ghost@c.org"]);
}

#[test]
fn a_name_inside_another_is_put_in_by_email() {
    let s = SuggestState {
        authors: Source::Ready(Arc::new(vec![
            person("Ann", "ann@a.org", 3),
            person("Anna Berg", "anna@b.org", 2),
            person("Bob", "joann@c.org", 1),
        ])),
        ..SuggestState::default()
    };
    let l = listing("author:", 7, &s);
    let inserts: Vec<&str> = l.items.iter().map(|i| i.insert.as_str()).collect();
    assert_eq!(
        inserts,
        ["author:<ann@a.org>", "author:\"Anna Berg\"", "author:Bob"]
    );
}

#[test]
fn forgotten_sources_are_read_again() {
    let mut s = state();
    let g = s.generation;
    s.forget_sources();
    assert!(s.source_idle(SourceKind::Authors) && s.source_idle(SourceKind::Paths));
    assert!(s.generation > g);
    s.chosen = Some(1);
    s.reset();
    assert!(s.chosen.is_none() && s.generation > g + 1);
}

#[test]
fn odd_file_names_are_put_in_as_literal_paths() {
    let s = SuggestState {
        paths: Source::Ready(Arc::new(PathIndex::new(vec![
            ":odd".to_owned(),
            "a[1].txt".to_owned(),
            "plain.txt".to_owned(),
        ]))),
        ..SuggestState::default()
    };
    let insert = |q: &str| listing(q, q.chars().count(), &s).items[0].insert.clone();
    assert_eq!(insert("path::od"), "path::(literal):odd");
    assert_eq!(insert("path:a["), "path::(literal)a[1].txt");
    assert_eq!(insert("path:pla"), "path:plain.txt");
}

#[test]
fn merges_is_followed_by_a_space() {
    let s = state();
    let l = listing("-mer", 4, &s);
    assert_eq!(
        accept("-mer", &l.range, &l.items[0]),
        ("-merges: ".to_owned(), 9)
    );
}

#[test]
fn lists_are_cached_until_the_query_cursor_or_sources_change() {
    let mut s = state();
    let first = s.listing_for("author:", 7);
    assert_eq!(first.items.len(), 3);
    s.authors = Source::Ready(Arc::new(Vec::new()));
    // Same query and cursor, sources not marked changed: the list from before.
    assert_eq!(s.listing_for("author:", 7), first);
    s.version += 1;
    assert!(s.listing_for("author:", 7).items.is_empty());
}

#[test]
fn folders_are_listed_once() {
    let index = PathIndex::new(vec!["a/b/c.txt".to_owned(), "a/b/d.txt".to_owned()]);
    let paths: Vec<&str> = index.entries.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["a/", "a/b/", "a/b/c.txt", "a/b/d.txt"]);
}
