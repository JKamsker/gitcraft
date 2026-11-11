use super::*;

fn editor() -> SplitEditor {
    let patch = "\
diff --git a/a.txt b/a.txt
index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644
--- a/a.txt
+++ b/a.txt
@@ -1,2 +1,2 @@
-one
+uno
 two
@@ -10,2 +10,2 @@
 nine
-ten
+diez
diff --git a/b.bin b/b.bin
index 1111111111111111111111111111111111111111..3333333333333333333333333333333333333333 100644
Binary files a/b.bin and b/b.bin differ
";
    let files = patchset::files_of(patch.as_bytes(), vec!["a.txt".into(), "b.bin".into()])
        .unwrap_or_default();
    SplitEditor::new(Read {
        hash: "c".into(),
        parent: "p".into(),
        message: "Subject over\ntwo lines\n\nBody line".into(),
        files,
    })
}

#[test]
fn everything_starts_in_the_last_commit_with_the_message_in_the_first() {
    let ed = editor();
    assert_eq!(ed.counts, [0, 5]);
    assert_eq!(ed.groups[0].subject, "Subject over two lines");
    assert_eq!(ed.groups[0].body, "Body line");
    assert_eq!(ed.summary, "Subject over two lines");
    // Said as a hint: that is how it starts.
    assert!(
        ed.problem()
            .is_some_and(|(p, hint)| p.contains("Commit 1 has no changes") && hint)
    );
    assert_eq!(ed.rows.len(), 1 + 8 + 1 + 1);
}

#[test]
fn files_hunks_and_lines_are_handed_out() {
    let mut ed = editor();
    // The first hunk's two lines.
    ed.assign_hunk(0, 0, 0);
    assert_eq!(ed.counts, [2, 3]);
    ed.assign_line(0, 1, 1);
    assert_eq!(ed.counts, [1, 4]);
    ed.assign_file(1, 0);
    assert_eq!(ed.counts, [2, 3]);
    assert!(ed.problem().is_some_and(|(p, _)| p.contains("summary")));
    ed.groups[1].subject = "Second".into();
    assert_eq!(ed.problem(), None);
    let patches = ed.cumulative_patches();
    assert_eq!(patches.len(), 1);
    // "uno" without the removal of "one", and the binary file.
    assert!(patches[0].contains("+uno") && patches[0].contains(" one"));
    assert!(!patches[0].contains("diez") && patches[0].contains("Binary files"));
    assert_eq!(
        ed.messages(),
        ["Subject over two lines\n\nBody line\n", "Second\n"]
    );
    // The last hunk runs to the end of the file; past it there is nothing.
    assert_eq!(ed.hunk_range(0, 4), 5..8);
    assert!(ed.hunk_range(9, 0).is_empty());
}

#[test]
fn a_third_commit_and_taking_one_out() {
    let mut ed = editor();
    ed.add_group();
    assert_eq!((ed.groups.len(), ed.active), (3, 2));
    ed.assign_file(1, 2);
    ed.assign_hunk(0, 0, 0);
    assert_eq!(ed.counts, [2, 2, 1]);
    let patches = ed.cumulative_patches();
    assert_eq!(patches.len(), 2);
    // The second holds what the first does, and more.
    assert!(patches[1].contains("+uno") && patches[1].contains("+diez"));
    ed.remove_group(1);
    assert_eq!(ed.counts, [4, 1]);
    assert_eq!(ed.active, 1);
    ed.remove_group(1);
    assert_eq!(ed.groups.len(), 2, "never fewer than two");
    for _ in 0..20 {
        ed.add_group();
    }
    assert_eq!(ed.groups.len(), MAX_GROUPS);
    // The first taken out: its changes go to the one after, now the first.
    let mut ed = editor();
    ed.add_group();
    ed.assign_hunk(0, 0, 0);
    ed.assign_file(1, 2);
    ed.remove_group(0);
    assert_eq!(ed.counts, [4, 1]);
}

#[test]
fn a_change_drops_what_was_checked_and_folding_lists_less() {
    let mut ed = editor();
    ed.checked = Some(Checked {
        trees: vec![],
        preview: Preview::new(&[]),
    });
    ed.assign_line(0, 1, 0);
    assert!(ed.checked.is_none());
    ed.busy = true;
    ed.assign_line(0, 2, 0);
    assert_eq!(ed.counts, [1, 4], "nothing changes while it is checked");
    ed.toggle_fold(0);
    assert_eq!(ed.rows, [Row::File(0), Row::File(1), Row::Line(1, 0)]);
    ed.toggle_fold(0);
    assert_eq!(ed.rows.len(), 11);
}
