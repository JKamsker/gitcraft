use super::*;

#[test]
#[allow(clippy::panic)] // Unexpected recorded commands must fail without executing Git.
fn only_confirmed_unborn_mutable_history_gets_the_empty_repository_explanation() {
    for at in [LinesAt::Index, LinesAt::WorkTree] {
        for probe in [
            Ok((1, String::new())),
            Ok((0, String::new())),
            Err("unreadable ref".into()),
        ] {
            let absent = matches!(&probe, Ok((1, _)));
            let mut calls = Vec::new();
            let result = resolve_line_commit(&at, |args, codes| {
                calls.push((
                    args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    codes.to_vec(),
                ));
                match args[0] {
                    "rev-parse" => Err("original resolution error".into()),
                    "symbolic-ref" => Ok((0, "refs/heads/main\n".into())),
                    "show-ref" => probe.clone(),
                    _ => panic!("unexpected command: {args:?}"),
                }
            });
            assert_eq!(calls.len(), 3);
            assert_eq!(
                calls[2],
                (
                    vec![
                        "show-ref".into(),
                        "--verify".into(),
                        "--quiet".into(),
                        "refs/heads/main".into()
                    ],
                    vec![0, 1]
                )
            );
            let expected = if absent {
                crate::i18n::text(
                    "backend-the-repository-has-no-commits-yet-so-these-lines-have-no-history-87624b",
                )
            } else {
                "original resolution error"
            };
            assert_eq!(result, Err(expected.to_owned()));
        }
        for symbolic in [
            Err("detached or inaccessible".into()),
            Ok((0, "refs/tags/unusual\n".into())),
        ] {
            let mut calls = 0;
            assert_eq!(
                resolve_line_commit(&at, |args, _| {
                    calls += 1;
                    match args[0] {
                        "rev-parse" => Err("original".into()),
                        "symbolic-ref" => symbolic.clone(),
                        _ => panic!("unexpected command: {args:?}"),
                    }
                }),
                Err("original".into())
            );
            assert_eq!(calls, 2);
        }
    }
    for at in [LinesAt::Rev("HEAD".into()), LinesAt::Rev("invalid".into())] {
        let mut calls = 0;
        assert_eq!(
            resolve_line_commit(&at, |args, _| {
                calls += 1;
                assert_eq!(args[0], "rev-parse");
                Err("explicit revision error".into())
            }),
            Err("explicit revision error".into())
        );
        assert_eq!(calls, 1);
    }
    let mut calls = 0;
    assert_eq!(
        resolve_line_commit(&LinesAt::WorkTree, |args, _| {
            calls += 1;
            assert_eq!(
                args,
                ["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"]
            );
            Ok((0, "captured-oid\n".into()))
        }),
        Ok("captured-oid\n".into())
    );
    assert_eq!(calls, 1);
}

#[test]
fn line_log_output_is_read_commit_by_commit() {
    let out = format!(
        "{RS}aaa{FS}Ann{FS}ann@x{FS}300{FS}Edit\n\ndiff --git a/new name.txt b/new name.txt\n\
         --- a/new name.txt\n+++ b/new name.txt\n@@ -4,3 +5,3 @@\n 4\n-6\n+six\n\
         {RS}mmm{FS}Max{FS}max@x{FS}250{FS}Merge\n\n\
         {RS}bbb{FS}Bob{FS}bob@x{FS}200{FS}Two hunks\n\ndiff --git a/old.txt b/old.txt\n\
         --- a/old.txt\n+++ b/old.txt\n@@ -2,1 +2,1 @@\n-x\n+y\n@@ -9 +9,2 @@\n-z\n+z\n+w\n\
         {RS}ccc{FS}Cy{FS}cy@x{FS}100{FS}Add\n\ndiff --git a/old.txt b/old.txt\n\
         --- /dev/null\n+++ b/old.txt\n@@ -0,0 +1,3 @@\n+a\n+b\n+c\n"
    );
    let h = parse_line_log(&out, "new name.txt");
    let summary: Vec<(&str, &str, Vec<LineRange>)> = h
        .iter()
        .map(|c| (c.hash.as_str(), c.path.as_str(), c.lines.clone()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("aaa", "new name.txt", vec![(5, 7)]),
            // No patch: the path of the commit after it.
            ("mmm", "new name.txt", vec![]),
            ("bbb", "old.txt", vec![(2, 2), (9, 10)]),
            ("ccc", "old.txt", vec![(1, 3)]),
        ]
    );
    assert_eq!((h[0].author.as_str(), h[0].time), ("Ann", 300));
    assert_eq!(h[2].summary, "Two hunks");
}

#[test]
fn a_record_separator_in_the_file_does_not_start_a_commit() {
    let out = format!(
        "{RS}aaa{FS}Ann{FS}a@x{FS}1{FS}S\n\ndiff --git a/f b/f\n--- a/f\n+++ b/f\n\
         @@ -1 +1 @@\n-{RS}old\n+{RS}new{FS}x\n"
    );
    let h = parse_line_log(&out, "f");
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].lines, vec![(1, 1)]);
}

#[test]
fn an_added_line_that_looks_like_a_file_header_is_not_one() {
    // The file's text has a line `++ b/evil`: added, git prints it `+++ b/evil`.
    let out = format!(
        "{RS}aaa{FS}Ann{FS}a@x{FS}1{FS}S\n\ndiff --git a/real.txt b/real.txt\n--- a/real.txt\n\
         +++ b/real.txt\n@@ -1,0 +2,1 @@\n+++ b/evil\n"
    );
    let h = parse_line_log(&out, "real.txt");
    assert_eq!(h[0].path, "real.txt");
    assert_eq!(h[0].lines, vec![(2, 2)]);
}

#[test]
fn the_tab_after_a_name_with_a_space_is_not_part_of_it() {
    let out = format!(
        "{RS}aaa{FS}Ann{FS}a@x{FS}1{FS}S\n\ndiff --git a/big: file.txt b/big: file.txt\n\
         --- a/big: file.txt\t\n+++ b/big: file.txt\t\n@@ -1 +1 @@\n-a\n+b\n"
    );
    assert_eq!(parse_line_log(&out, "x")[0].path, "big: file.txt");
}

#[test]
fn nothing_printed_is_no_commits() {
    assert!(parse_line_log("", "f").is_empty());
    assert!(parse_line_log("\n\n", "f").is_empty());
}

#[test]
fn quoted_names_are_read_back() {
    assert_eq!(unquote_path("b/plain name.txt"), "b/plain name.txt");
    assert_eq!(unquote_path("\"b/q\\\"t\\tx.txt\""), "b/q\"t\tx.txt");
    assert_eq!(unquote_path("\"b/\\303\\274ber.txt\""), "b/über.txt");
    assert_eq!(unquote_path("\"b/back\\\\slash\""), "b/back\\slash");
}

#[test]
fn lines_on_disk_are_found_in_the_commit() {
    // Line 1 added at the top, line 6 (old 5) changed, two lines added after old 8.
    let diff = "@@ -0,0 +1 @@\n+top\n@@ -5 +6 @@\n-five\n+FIVE\n@@ -8,0 +10,2 @@\n+n1\n+n2\n";
    // Lines below everything shift by the three added.
    assert_eq!(map_to_old(diff, (12, 13)), Some((9, 10)));
    // Line 2 was line 1.
    assert_eq!(map_to_old(diff, (2, 3)), Some((1, 2)));
    // The added top line is left out; the changed line 6 too.
    assert_eq!(map_to_old(diff, (1, 6)), Some((1, 4)));
    // Only added lines: nothing to follow.
    assert_eq!(map_to_old(diff, (1, 1)), None);
    assert_eq!(map_to_old(diff, (10, 11)), None);
    // Line 9 is old 8, the lines added after it don't move it.
    assert_eq!(map_to_old(diff, (9, 9)), Some((8, 8)));
    // No changes: the same lines.
    assert_eq!(map_to_old("", (4, 9)), Some((4, 9)));
}

#[test]
fn removed_lines_move_the_ones_after_them_up() {
    // Old lines 2–4 removed.
    let diff = "@@ -2,3 +1,0 @@\n-a\n-b\n-c\n";
    assert_eq!(map_to_old(diff, (1, 1)), Some((1, 1)));
    assert_eq!(map_to_old(diff, (2, 3)), Some((5, 6)));
}

#[test]
fn ranges_are_put_in_order() {
    assert_eq!(ordered((9, 3)), (3, 9));
    assert_eq!(ordered((0, 0)), (1, 1));
    assert_eq!(line_spec((3, 9), "a:b, c.txt"), "-L3,9:a:b, c.txt");
}

#[test]
fn hunk_headers_with_and_without_counts() {
    assert_eq!(hunk_ranges("@@ -4,3 +5,3 @@"), Some(((4, 3), (5, 3))));
    assert_eq!(hunk_ranges("@@ -9 +9,2 @@ fn x"), Some(((9, 1), (9, 2))));
    assert_eq!(hunk_ranges("@@ -0,0 +1 @@"), Some(((0, 0), (1, 1))));
    assert_eq!(hunk_ranges(" 4"), None);
}
