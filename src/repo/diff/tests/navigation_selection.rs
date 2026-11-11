#[test]
fn changes_group_into_runs_per_kind() {
    let (n, d, a, both) = ((false, false), (true, false), (false, true), (true, true));
    let runs = change_runs([n, d, d, a, n, both, a].into_iter());
    assert_eq!(runs.removed, vec![1..3, 5..6]);
    assert_eq!(runs.added, vec![3..4, 5..7]);
}

#[test]
fn removals_and_additions_next_to_each_other_are_one_change() {
    let (n, d, a, both) = ((false, false), (true, false), (false, true), (true, true));
    let runs = change_runs([n, d, d, a, n, both, a, n, n, a].into_iter());
    assert_eq!(changes(&runs), vec![1..4, 5..7, 9..10]);
    assert!(changes(&change_runs([n, n].into_iter())).is_empty());
}

#[test]
fn stepping_goes_from_the_change_jumped_to_or_from_what_is_in_view() {
    let list = vec![1..4, 20..22, 40..41];
    // At the top: the first change is in view already, so the next one is the second.
    assert_eq!(step(&list, 0.0, None, 1), Some(1));
    assert_eq!(step(&list, 0.0, None, -1), Some(0));
    // Scrolled to the second change: on to the third, back to the first.
    let at = offset_of(20);
    assert_eq!(step(&list, at, None, 1), Some(2));
    assert_eq!(step(&list, at, None, -1), Some(0));
    // The view couldn't scroll as far as the last change jumped to: still steps from it.
    assert_eq!(step(&list, 300.0, Some((1, 300.0)), 1), Some(2));
    assert_eq!(step(&list, 300.0, Some((2, 300.0)), 1), None);
    assert_eq!(step(&list, 300.0, Some((0, 300.0)), -1), None);
    // Scrolled by hand since: from what is in view again.
    assert_eq!(step(&list, 0.0, Some((2, 300.0)), -1), Some(0));
    assert_eq!(step(&[], 0.0, None, 1), None);
}

#[test]
fn a_click_on_a_change_mark_finds_its_change() {
    let list = vec![1..4, 20..22];
    assert_eq!(change_at(&list, 2.0, 0.5), Some(0));
    assert_eq!(change_at(&list, 22.3, 0.5), Some(1));
    assert_eq!(change_at(&list, 10.0, 0.5), None);
}

#[test]
fn selected_text_follows_the_side_it_was_made_on() {
    // 0 hunk, 1 " a", 2 "-let x = 1;", 3 "-gone", 4 "+let x = 2;", 5 " b"
    let lines = parse("@@ -1,3 +1,3 @@\n a\n-let x = 1;\n-gone\n+let x = 2;\n b\n");
    let sel = |side, anchor, cursor| TextSel {
        side,
        anchor,
        cursor,
    };
    // Dragged backwards, from "let x" in the removal to the end of "a".
    let unified = sel(Side::Unified, (2, 5), (1, 1));
    assert_eq!(unified.text(&lines, &Default::default()), "\nlet x");
    // On the new side the removals are not there.
    let new = sel(Side::New, (1, 0), (5, 1));
    assert_eq!(new.text(&lines, &Default::default()), "a\nlet x = 2;\nb");
    assert_eq!(new.chars_of(4, Side::New), Some(0..usize::MAX));
    assert_eq!(new.chars_of(4, Side::Old), None);
    assert_eq!(new.chars_of(5, Side::New), Some(0..1));
    // Pressed and not moved: nothing.
    assert_eq!(sel(Side::Old, (2, 3), (2, 3)).chars_of(2, Side::Old), None);
}

#[test]
fn a_line_cut_to_be_shown_is_copied_whole() {
    use crate::repo::highlight::{MAX_LINE_CHARS, cap_line};
    let whole = format!("let x = \"{}\";", "y".repeat(MAX_LINE_CHARS));
    let mut lines = parse(&format!("@@ -1,2 +1,2 @@\n a\n+{whole}\n"));
    let uncut: std::collections::HashMap<usize, String> = lines
        .iter_mut()
        .enumerate()
        .filter_map(|(i, l)| Some((i, cap_line(&mut l.text, None)?)))
        .collect();
    assert_eq!(uncut.get(&2), Some(&whole));
    let shown = lines[2].text.chars().count();
    let sel = |anchor, cursor| TextSel {
        side: Side::Unified,
        anchor,
        cursor,
    };
    // Through the end of what is shown (the note included): the whole line.
    assert_eq!(
        sel((1, 0), (2, shown)).text(&lines, &uncut),
        format!("a\n{whole}")
    );
    // From inside the note: from the cut on, to the end.
    let rest: String = whole.chars().skip(MAX_LINE_CHARS).collect();
    assert_eq!(
        sel((2, MAX_LINE_CHARS + 3), (2, shown)).text(&lines, &uncut),
        rest
    );
    // Within what is shown: just that.
    assert_eq!(sel((2, 0), (2, 3)).text(&lines, &uncut), "let");
}

#[test]
fn numbers_follow_hunk_headers() {
    let text = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -3,3 +3,4 @@ fn x\n a\n-b\n+c\n+d\n e\n\\ No newline at end of file\n";
    let lines = parse(text);
    let summary: Vec<_> = lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
    assert_eq!(
        summary,
        vec![
            (Kind::Hunk, None, None),
            (Kind::Context, Some(3), Some(3)),
            (Kind::Del, Some(4), None),
            (Kind::Add, None, Some(4)),
            (Kind::Add, None, Some(5)),
            (Kind::Context, Some(5), Some(6)),
            (Kind::Meta, None, None),
        ]
    );
    assert_eq!(lines[3].text, "c");
}

/// The function name git prints after a hunk's ranges can hold `-` and `+` words.
#[test]
fn numbers_ignore_the_hunk_headers_function_name() {
    assert_eq!(hunk_start("@@ -13,7 +14,7 @@ Hello - world +x"), (12, 13));
    assert_eq!(hunk_start("@@ -1 +1 @@"), (0, 0));
    assert_eq!(hunk_start("@@@ -1,1 -2,1 +3,5 @@@ a -9"), (1, 2));
    let lines = parse("@@ -13,2 +13,2 @@ int f(int a, -1)\n a\n-b\n+c\n");
    assert_eq!(lines[1].old, Some(13));
    assert_eq!(lines[2].old, Some(14));
    assert_eq!(lines[3].new, Some(14));
}

/// A hunk header that claims to start at the last line number there is: the numbers stop
/// there rather than overflow.
#[test]
fn numbers_past_the_largest_stay_there() {
    let lines = parse("@@ -4294967295 +4294967295 @@\n a\n b\n-c\n+d\n");
    let numbers: Vec<_> = lines.iter().map(|l| (l.old, l.new)).collect();
    let max = Some(u32::MAX);
    assert_eq!(
        numbers,
        vec![
            (None, None),
            (max, max),
            (max, max),
            (max, None),
            (None, max)
        ]
    );
}

/// The last line changed in a file without a final newline: its old and new versions are
/// still side by side, the note after them.
#[test]
fn no_newline_note_keeps_the_last_lines_paired() {
    let lines = parse(
        "@@ -1,2 +1,2 @@\n a\n-let x = 1;\n\\ No newline at end of file\n+let x = 2;\n\\ No newline at end of file\n",
    );
    let (changed, split) = pair_lines(&lines);
    assert_eq!(
        split,
        vec![
            (Some(0), Some(0)),
            (Some(1), Some(1)),
            (Some(2), Some(4)),
            (Some(3), Some(3)),
            (Some(5), Some(5)),
        ]
    );
    assert_eq!(changed[2], Some((8, 9)));
    assert_eq!(changed[4], Some((8, 9)));
}

#[test]
fn selection_belongs_to_the_layout_it_was_made_in() {
    let sel = |side| TextSel {
        side,
        anchor: (1, 0),
        cursor: (2, 1),
    };
    assert!(sel(Side::Unified).made_in(false));
    assert!(!sel(Side::Unified).made_in(true));
    assert!(sel(Side::Old).made_in(true));
    assert!(!sel(Side::New).made_in(false));
}

#[test]
fn images_are_named_by_their_contents() {
    let (a, b): (&[u8], &[u8]) = (b"one", b"two");
    assert_eq!(image_id(Some(a), Some(b)), image_id(Some(a), Some(b)));
    assert_ne!(image_id(Some(a), Some(b)), image_id(Some(a), Some(a)));
    assert_ne!(image_id(None, Some(a)), image_id(Some(a), None));
}

#[test]
fn combined_conflict_diff() {
    let text = "@@@ -1,1 -1,1 +1,5 @@@\n++<<<<<<< HEAD\n +ours\n  same\n";
    let lines = parse(text);
    assert_eq!(lines[1].kind, Kind::Add);
    assert_eq!(lines[1].text, "<<<<<<< HEAD");
    assert_eq!(lines[3].kind, Kind::Context);
    assert_eq!(lines[3].old, None);
}

/// A new file shows unified even side by side: it has no old side to put beside it.
#[test]
fn new_files_are_told_apart() {
    let added = "diff --git a/x b/x\nnew file mode 100644\nindex 0000000..e69de29\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+a\n";
    let untracked = "diff --git a/x b/x\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+a\n";
    let changed = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n--- /dev/null\n+new file mode 1\n";
    let deleted =
        "diff --git a/x b/x\ndeleted file mode 100644\n--- a/x\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n";
    assert!(is_new_file(added));
    assert!(is_new_file(untracked));
    // The same words as lines of the file don't count.
    assert!(!is_new_file(changed));
    assert!(!is_new_file(deleted));
}

#[test]
fn binary_notice() {
    let lines = parse("diff --git a/x b/x\nBinary files a/x and b/x differ\n");
    assert_eq!(lines, vec![meta("Binary files a/x and b/x differ")]);
}

#[test]
fn side_by_side_pairs_and_inline_changes() {
    let lines = parse("@@ -1,3 +1,3 @@\n a\n-let x = 1;\n-gone\n+let x = 2;\n b\n");
    let (changed, split) = pair_lines(&lines);
    // hunk, a, (del x1, add x2), (del gone, —), b
    assert_eq!(
        split,
        vec![
            (Some(0), Some(0)),
            (Some(1), Some(1)),
            (Some(2), Some(4)),
            (Some(3), None),
            (Some(5), Some(5))
        ]
    );
    assert_eq!(changed[2], Some((8, 9)));
    assert_eq!(changed[4], Some((8, 9)));
    assert_eq!(changed[3], None);
}

#[test]
fn rewrites_are_not_marked_inline() {
    assert_eq!(changed_ranges("completely", "different words"), None);
    assert_eq!(changed_ranges("abc", "abXc"), Some(((2, 2), (2, 3))));
}

const PATCH: &str = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n\
    @@ -1,4 +1,4 @@ top\n a\n-b\n-c\n+B\n+C\n d\n\
    @@ -10,2 +10,3 @@\n x\n+y\n z\n";
