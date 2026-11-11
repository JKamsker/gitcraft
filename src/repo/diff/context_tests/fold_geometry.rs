#[test]
fn header_numbers_read_counts_and_the_function() {
    assert_eq!(
        header_numbers("@@ -12,5 +14,7 @@ fn x()"),
        Some((12, 5, 14, 7, " fn x()"))
    );
    assert_eq!(header_numbers("@@ -3 +3,0 @@"), Some((3, 1, 3, 0, "")));
    assert_eq!(header_numbers("@@@ -1,1 -1,1 +1,1 @@@"), None);
    assert_eq!(
        function_of("@@ -1,2 +1,2 @@   impl Foo {  "),
        Some("impl Foo {")
    );
    assert_eq!(function_of("@@ -1,2 +1,2 @@"), None);
}

#[test]
fn gaps_lie_between_the_hunks_and_after_the_last() {
    // 30 lines; line 5 changed, line 20 removed.
    let text = "@@ -2,7 +2,7 @@\n a\n b\n c\n-d\n+D\n e\n f\n g\n@@ -17,7 +17,6 @@\n h\n i\n j\n-k\n l\n m\n n\n";
    let lines = parse(text);
    let gaps = gaps_of(&lines, 29).unwrap_or_default();
    let summary: Vec<_> = gaps
        .iter()
        .map(|g| (g.first, g.last, g.old_offset, g.trailing))
        .collect();
    assert_eq!(
        summary,
        vec![(1, 1, 0, false), (9, 16, 0, false), (23, 29, 1, true)]
    );
    assert_eq!(gaps[0].row, 0);
    assert_eq!(gaps[1].row, 9);
    assert_eq!(gaps[2].row, lines.len());
    // A hunk adding lines to an empty range numbers it by the line before.
    let added = parse("@@ -4,0 +5,2 @@\n+x\n+y\n");
    let gaps = gaps_of(&added, 10).unwrap_or_default();
    assert_eq!((gaps[0].first, gaps[0].last), (1, 4));
    assert_eq!(
        (gaps[1].first, gaps[1].last, gaps[1].old_offset),
        (7, 10, -2)
    );
    assert!(gaps_of(&parse("@@@ -1,1 -1,1 +1,1 @@@\n  a\n"), 1).is_none());
    assert_eq!(gaps_of(&[], 5), Some(Vec::new()));
}

#[test]
fn the_enclosing_function_runs_from_its_header_to_its_end() {
    let file =
        "use x;\n\nfn outer(a: u32) -> u32 {\n    let b = a;\n\n    b + 1\n}\n\nfn next() {}\n";
    let ctx = Context {
        raw: file.lines().map(str::to_owned).collect(),
        path: "a.rs".to_owned(),
        dark: true,
        no_newline_at_end: false,
        gaps: Vec::new(),
    };
    assert_eq!(
        ctx.function_span("@@ -5,2 +5,2 @@ fn outer(a: u32) -> u32 {", 5),
        Some((3, 7))
    );
    // Python: no closing line, the function ends before the next one at its depth.
    let py = "class A:\n    def f(self):\n        x = 1\n        return x\n    def g(self):\n        pass\n";
    let ctx = Context {
        raw: py.lines().map(str::to_owned).collect(),
        ..ctx
    };
    assert_eq!(
        ctx.function_span("@@ -3,1 +3,1 @@ def f(self):", 3),
        Some((2, 4))
    );
    assert_eq!(ctx.function_span("@@ -3,1 +3,1 @@ def missing():", 3), None);
}

#[test]
fn unfolding_rebuilds_populated_hunk_and_change_caches_in_both_layouts() -> Result<(), String> {
    let lines = parse("@@ -1 +1 @@\n+a\n@@ -3 +3 @@\n+c\n");
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "synthetic".into(),
        path: "f.txt".into(),
    });
    view.data = Some(Ok(DiffData {
        split: crate::repo::diff::comparison_pairs(&lines),
        spans: vec![None; lines.len()],
        changed: vec![None; lines.len()],
        lines,
        context: Some(Context {
            raw: vec!["a".into(), "b".into(), "c".into()],
            gaps: vec![Gap {
                row: 0,
                first: 1,
                last: 0,
                old_offset: 0,
                trailing: false,
                function: None,
            }, Gap {
                row: 2,
                first: 2,
                last: 2,
                old_offset: 0,
                trailing: false,
                function: None,
            }],
            ..Default::default()
        }),
        ..Default::default()
    }));
    let d = view.data.as_ref().ok_or("missing data")?.as_ref().map_err(Clone::clone)?;
    for split in [false, true] {
        assert_eq!(d.hunk_at(split, 3), Some(2));
        assert_eq!(d.runs(split).0.added, vec![1..2, 3..4]);
    }
    view.keyboard_hunk = Some(2);
    view.chosen.insert(3);
    view.anchor = Some(3);
    view.jump_line = Some(3);
    view.wrap_offsets = Some(std::sync::Arc::new(vec![0.0, 19.0, 190.0]));
    view.unfold(1, Unfold::Down(1));
    let d = view.data.as_ref().ok_or("missing data")?.as_ref().map_err(Clone::clone)?;
    for split in [false, true] {
        assert_eq!(d.hunk_at(split, 4), Some(3));
        assert_eq!(d.runs(split).0.added, vec![1..2, 4..5]);
    }
    assert_eq!(view.keyboard_hunk, Some(3));
    assert_eq!(view.chosen.iter().copied().collect::<Vec<_>>(), [4]);
    assert_eq!(view.anchor, Some(4));
    assert_eq!(view.jump_line, Some(4));
    assert!(view.wrap_offsets.is_none());
    Ok(())
}

include!("geometry_cache.rs");
