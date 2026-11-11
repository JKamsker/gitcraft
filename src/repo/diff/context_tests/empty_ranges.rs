/// Opening down below a hunk that only removed lines numbers its header from the line after
/// the empty range, and the patch still applies.
#[test]
fn opening_below_a_hunk_with_an_empty_side_numbers_it_right() -> Result<(), String> {
    let base = numbered(40, "");
    let dir = repo("empty-side", "f.txt", &base);
    // Lines 10 and 11 removed, nothing else: `@@ -7,8 +7,6 @@` with -U3; with no context the
    // new side is empty. Use the user's `diff.context=0`-like shape by removing at the top.
    let edited: String = base
        .lines()
        .enumerate()
        .filter(|(k, _)| *k != 9 && *k != 10)
        .map(|(_, l)| format!("{l}\n"))
        .collect();
    let _ = std::fs::write(dir.join("f.txt"), edited);
    let key = unstaged("f.txt");
    let mut view = view_of(&dir, &key)?;
    view.unfold(1, Unfold::Down(5));
    let d = data(&view)?;
    let header = d.lines[0].raw.clone();
    let hunk = hunk_changes(&d.lines, 0);
    apply(&dir, &view, &hunk, Pick::Stage)?;
    let _ = std::fs::remove_dir_all(&dir);
    // Staged: the two lines are gone from the index.
    assert!(header.starts_with("@@ -7,"), "{header}");
    Ok(())
}

#[test]
fn header_counts_from_an_empty_range_after_opening_down() {
    let lines = parse("@@ -4,0 +5,2 @@\n+x\n+y\n");
    let mut data = data_from(lines);
    data.context = Some(Context {
        gaps: gaps_of(&data.lines, 12).unwrap_or_default(),
        raw: (1..=12)
            .map(|n| {
                if n == 5 || n == 6 {
                    "new".to_owned()
                } else {
                    format!("l{n}")
                }
            })
            .collect(),
        path: "f.txt".to_owned(),
        dark: true,
        no_newline_at_end: false,
    });
    data.unfold(1, Unfold::Down(3));
    assert_eq!(data.lines[0].raw, "@@ -5,3 +5,5 @@");
    assert_eq!(data.lines[3].old, Some(5));
    assert_eq!(data.lines[3].new, Some(7));
}

/// A hunk past the end of the function git names has no _Function_ to open.
#[test]
fn no_function_button_when_the_named_line_does_not_enclose_the_hunk() {
    let file = "fn first() {\n    1\n}\n\nstatic X: u8 = 1;\nstatic Y: u8 = 2;\n";
    let lines = parse("@@ -5,1 +5,1 @@ fn first() {\n-static X: u8 = 0;\n+static X: u8 = 1;\n");
    let mut ctx = Context {
        raw: file.lines().map(str::to_owned).collect(),
        path: "a.rs".to_owned(),
        dark: true,
        no_newline_at_end: false,
        gaps: gaps_of(&lines, 6).unwrap_or_default(),
    };
    ctx.gaps[0].function = ctx.function_for(&lines, 0);
    assert_eq!(ctx.gaps[0].function, None);
    assert!(!ctx.function_opens(0));
}

fn data_from(lines: Vec<DiffLine>) -> DiffData {
    let (changed, split) = crate::repo::diff::parse::pair_lines(&lines);
    DiffData {
        spans: vec![None; lines.len()],
        changed,
        split,
        image: None,
        lfs: None,
        header: String::new(),
        partial: false,
        moves: Default::default(),
        context: None,
        new_file: false,
        longest: 0,
        source: 0,
        uncut: Default::default(),
        map: Default::default(),
        lines,
        ..Default::default()
    }
}

/// A moved block ending its hunk (no context after it) keeps its lines when lines are opened
/// right after it.
#[test]
fn a_moved_block_does_not_grow_over_lines_opened_after_it() {
    let lines = parse(
        "@@ -11,3 +10,0 @@\n-let first_moved_line = compute_it();\n-let second_moved_line = again();\n-let third = more_letters_here();\n@@ -40,0 +38,3 @@\n+let first_moved_line = compute_it();\n+let second_moved_line = again();\n+let third = more_letters_here();\n",
    );
    let mut data = data_from(lines);
    data.moves = crate::repo::diff::moved::detect(&data.lines);
    assert_eq!(data.moves.blocks.len(), 1);
    let before = data.moves.blocks[0].clone();
    data.context = Some(Context {
        gaps: gaps_of(&data.lines, 60).unwrap_or_default(),
        raw: (1..=60).map(|n| format!("l{n}")).collect(),
        path: "f.rs".to_owned(),
        dark: true,
        no_newline_at_end: false,
    });
    data.unfold(1, Unfold::Down(2));
    let after = &data.moves.blocks[0];
    assert_eq!(after.del, before.del);
    assert_eq!(after.add.len(), before.add.len());
    assert_eq!(data.lines[before.del.end].kind, Kind::Context);
    // The header below the empty range numbers the lines opened from the line after it.
    assert_eq!(data.lines[0].raw, "@@ -11,5 +11,2 @@");
}
