use super::*;
#[test]
fn renamed_files_follow_insertions_but_changed_lines_do_not() -> Result<(), String> {
    let r = crate::git::stack::test_repo::TestRepo::new("review-anchor-relocation")?;
    let old = r.commit("old.rs", "one\ntwo\nthree\nfour\nfive\nsix\n", "old")?;
    r.git(&["mv", "old.rs", "new.rs"])?;
    let new = r.commit(
        "new.rs",
        "inserted\none\ntwo\nchanged\nfour\nfive\nsix\n",
        "rewrite",
    )?;
    let mut a = Anchor {
        path: "old.rs".into(),
        line: 2,
        start: None,
        right: true,
    };
    let (mapped, _) = relocate(r.path(), &old, &new, &a)?.ok_or("missing relocation")?;
    assert_eq!((mapped.path.as_str(), mapped.line), ("new.rs", 3));
    a.line = 3;
    assert!(relocate(r.path(), &old, &new, &a)?.is_none());
    // Local evidence round-trips independently of forge objects and survives reopening.
    store::save(r.path(), "anchor-test", "https://h/o/r#1", &a)?;
    let saved = store::load::<Anchor>(r.path(), "anchor-test", "https://h/o/r#1")?
        .ok_or("missing saved anchor")?;
    assert_eq!(saved.line, 3);
    assert!(store::load::<Anchor>(r.path(), "anchor-test", "other")?.is_none());
    Ok(())
}

#[test]
fn anchors_follow_only_unchanged_lines() {
    let patch = "@@ -0,0 +1,2 @@\n@@ -4,2 +6,1 @@\n@@ -9 +9,0 @@";
    assert_eq!(map_line(patch, 1), Some(3));
    assert_eq!(map_line(patch, 3), Some(5));
    assert_eq!(map_line(patch, 4), None);
    assert_eq!(map_line(patch, 5), None);
    assert_eq!(map_line(patch, 6), Some(7));
    assert_eq!(map_line(patch, 9), None);
    assert_eq!(map_line(patch, 10), Some(10));
    assert_eq!(map_line("@@ -3,0 +4,2 @@", 3), Some(3));
    assert_eq!(map_line("@@ -3,0 +4,2 @@", 4), Some(6));
    assert_eq!(map_line("", 0), None);
}
#[test]
fn r2_pure_multiline_ranges_survive_serialization_and_repeated_rewrites() -> Result<(), String> {
    let (start, line) = mapped_span("@@ -0,0 +1 @@", Some(2), 3).ok_or("first mapping")?;
    let anchor = Anchor {
        path: "a.rs".into(),
        line,
        start,
        right: true,
    };
    let bytes = serde_json::to_vec(&anchor).map_err(|e| e.to_string())?;
    let reopened: Anchor = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    assert_eq!((reopened.start, reopened.line), (Some(3), 4));
    assert!(mapped_span("@@ -3 +3 @@", reopened.start, reopened.line).is_none());
    assert_eq!(
        mapped_span("@@ -0,0 +1,2 @@", reopened.start, reopened.line),
        Some((Some(5), 6))
    );
    Ok(())
}
#[test]
fn r2_pure_relocation_maps_cannot_cross_generations_or_errors() {
    let mut map = Relocations {
        base: "A".into(),
        head: "B".into(),
        ..Default::default()
    };
    assert!(map.current("A", "B").is_some());
    assert!(map.current("A", "C").is_none());
    map.error = Some("unavailable".into());
    assert!(map.current("A", "B").is_none());
    assert!(Relocations::default().current("A", "B").is_none());
}
