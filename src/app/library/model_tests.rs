use super::*;

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

fn done(repos: &[&str], outcome: Outcome) -> ScanResult {
    ScanResult {
        repos: repos.iter().map(|s| p(s)).collect(),
        outcome,
        at: 100,
    }
}

fn paths(lib: &Library) -> Vec<String> {
    let mut v: Vec<_> = lib
        .entries
        .iter()
        .map(|e| e.path.to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Overlapping roots and repeat scans give one row per checkout; removing one source keeps
/// what another still covers.
#[test]
fn overlapping_sources_share_rows() {
    let mut lib = Library::default();
    let a = lib.add_root(p("/work"), 2);
    let b = lib.add_root(p("/work/projects"), 1);
    assert_eq!(
        lib.add_root(p("/work/"), 3),
        a,
        "the same folder is one root"
    );
    let (ra, rb) = (Source::Root(a), Source::Root(b));
    lib.finish(
        &ra,
        done(&["/work/projects/app", "/work/x"], Outcome::Complete),
    );
    lib.finish(
        &rb,
        done(
            &["/work/projects/app/", "/work/projects/tool"],
            Outcome::Complete,
        ),
    );
    lib.finish(
        &rb,
        done(
            &["/work/projects/app", "/work/projects/tool"],
            Outcome::Complete,
        ),
    );
    assert_eq!(
        paths(&lib),
        ["/work/projects/app", "/work/projects/tool", "/work/x"]
    );
    let app = &lib.entries[0];
    assert_eq!(app.sources, BTreeSet::from([ra.clone(), rb.clone()]));
    assert_eq!(lib.count(&rb), 2);
    lib.remove_root(b);
    assert_eq!(paths(&lib), ["/work/projects/app", "/work/x"]);
    assert_eq!(lib.entries[0].sources, BTreeSet::from([ra.clone()]));
    lib.set_enabled(a, false);
    assert!(lib.entries.is_empty());
    assert!(!lib.has_source(&ra));
    assert_eq!(lib.roots.len(), 1, "turned off, not removed");
}

/// A complete rescan drops what is gone; a partial or failed one keeps it as stale.
#[test]
fn incomplete_scans_never_delete() {
    let mut lib = Library::default();
    let id = lib.add_root(p("/r"), 1);
    let src = Source::Root(id);
    lib.finish(&src, done(&["/r/a", "/r/b"], Outcome::Complete));
    lib.finish(&src, done(&["/r/a"], Outcome::Partial(Stopped::Time)));
    assert_eq!(paths(&lib), ["/r/a", "/r/b"]);
    assert!(!lib.entries[0].stale());
    assert!(lib.entries[1].stale());
    lib.finish(&src, done(&[], Outcome::Failed("gone".into())));
    assert!(lib.entries.iter().all(Entry::stale));
    let last = lib.root(id).and_then(|r| r.last.clone());
    assert_eq!(
        last.map(|l| l.outcome),
        Some(Outcome::Failed("gone".into()))
    );
    // Seen again: confirmed again.
    lib.finish(&src, done(&["/r/b"], Outcome::Partial(Stopped::Asked)));
    assert!(!lib.entries[1].stale());
    lib.finish(&src, done(&["/r/b"], Outcome::Complete));
    assert_eq!(paths(&lib), ["/r/b"]);
}

/// Hidden paths stay hidden whatever finds them again; nearby folders keep the latest five.
#[test]
fn hidden_paths_and_the_latest_nearby_folders() {
    let mut lib = Library::default();
    lib.hide(Path::new("/w/secret"));
    let id = lib.add_root(p("/w"), 1);
    lib.finish(
        &Source::Root(id),
        done(&["/w/secret", "/w/ok"], Outcome::Complete),
    );
    assert!(lib.is_hidden(Path::new("/w/secret/")));
    assert!(!lib.is_hidden(Path::new("/w/ok")));
    lib.remove_root(id);
    let id = lib.add_root(p("/w"), 1);
    lib.finish(&Source::Root(id), done(&["/w/secret"], Outcome::Complete));
    assert!(
        lib.is_hidden(Path::new("/w/secret")),
        "a root change can't bring it back"
    );

    for i in 0..5 {
        let parent = p(&format!("/n{i}"));
        assert!(lib.note_nearby(parent.clone()).is_empty());
        lib.finish(
            &Source::Nearby(parent),
            done(&[&format!("/n{i}/r")], Outcome::Complete),
        );
    }
    // Opened again: first again, nothing pushed out.
    assert!(lib.note_nearby(p("/n0")).is_empty());
    assert_eq!(lib.note_nearby(p("/n5")), [p("/n1")]);
    assert_eq!(lib.nearby.len(), MAX_NEARBY);
    assert_eq!(lib.nearby[0].parent, p("/n5"));
    assert!(!paths(&lib).contains(&"/n1/r".to_owned()));
    assert!(lib.has_source(&Source::Nearby(p("/n0"))));
    lib.set_nearby_enabled(false);
    assert!(lib.nearby.is_empty());
    assert!(
        lib.entries
            .iter()
            .all(|e| !matches!(e.sources.first(), Some(Source::Nearby(_))))
    );
    // A late result for a source no longer there records nothing.
    lib.finish(&Source::Nearby(p("/n2")), done(&[], Outcome::Complete));
    assert!(lib.nearby.is_empty());
}

/// The library keeps at most [`MAX_INDEX`] repositories.
#[test]
fn the_index_is_bounded() {
    let mut lib = Library::default();
    let id = lib.add_root(p("/big"), 1);
    let many: Vec<PathBuf> = (0..MAX_INDEX + 10)
        .map(|i| p(&format!("/big/{i}")))
        .collect();
    assert_eq!(lib.add_found(&Source::Root(id), &many), MAX_INDEX);
    assert_eq!(lib.entries.len(), MAX_INDEX);
}

/// A saved library from before any field existed, or with fields unknown, reads with safe
/// defaults; a saved one reads back the same.
#[test]
fn saved_libraries_read_back() {
    let old: Library = ron::from_str("()").unwrap_or_default();
    assert_eq!(old, Library::default());
    assert!(old.nearby_enabled);
    let partial: Library = ron::from_str("(roots: [(id: 3, path: \"/x\")])").unwrap_or_default();
    assert_eq!(partial.roots[0].depth, 1);
    assert!(partial.roots[0].enabled);
    let mut lib = Library::default();
    let id = lib.add_root(p("/my projects/Größe 日本"), 4);
    lib.finish(
        &Source::Root(id),
        done(
            &["/my projects/Größe 日本/a"],
            Outcome::Partial(Stopped::Repos),
        ),
    );
    lib.hide(Path::new("/h"));
    let text = ron::to_string(&lib).unwrap_or_default();
    let back: Library = ron::from_str(&text).unwrap_or_default();
    assert_eq!(back, lib);
}
