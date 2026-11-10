/// The key of `n` lines of a file view, read into `at`.
fn file_key(path: &str, at: usize, n: usize) -> super::SourceKey {
    super::SourceKey {
        what: super::Source::File {
            path: path.into(),
            rev: None,
        },
        at,
        len: n,
    }
}

#[test]
fn lines_read_again_keep_the_current_match_and_another_view_starts_at_its_top() {
    let ctx = egui::Context::default();
    let lines: Vec<String> = (0..100).map(|i| format!("x{i}")).collect();
    let mut find = Find::default();
    find.show();
    find.query = query("x");
    let search = |find: &mut Find, key| {
        find.update(
            &ctx,
            key,
            100,
            |i| super::nth(&lines, i),
            super::Layout::Lines,
        );
    };
    search(&mut find, file_key("a", 1, 100));
    for _ in 0..7 {
        find.step(1);
    }
    let _ = find.take_reveal();
    find.saw(Seen {
        y: 40.0 * 19.0,
        height: 190.0,
        text_w: 500.0,
        line_h: 19.0,
        x: 0.0,
    });
    // The same file read again (saved elsewhere): the same match, no scrolling.
    search(&mut find, file_key("a", 2, 100));
    assert_eq!(find.current().map(|h| h.line), Some(7));
    assert!(find.take_reveal().is_none());
    // Another file: from its top, not from where the last one was scrolled.
    search(&mut find, file_key("b", 3, 100));
    assert_eq!(find.current().map(|h| h.line), Some(0));
    assert!(find.take_reveal().is_some());
}

#[test]
fn results_go_once_nothing_shown_is_searched() {
    let ctx = egui::Context::default();
    let lines = ["needle"];
    let mut find = Find::default();
    find.show();
    find.query = query("needle");
    find.update(
        &ctx,
        file_key("a", 1, 1),
        1,
        |i| super::nth(&lines, i),
        super::Layout::Lines,
    );
    // The bar of the next frame: searched, so kept.
    find.drop_unsearched();
    assert_eq!(find.count(), 1);
    // An image took the file's place: nothing searched before the bar of the frame after.
    find.drop_unsearched();
    assert_eq!(
        (find.count(), find.summary(), find.current()),
        (0, None, None)
    );
    find.step(1);
    assert!(find.current().is_none());
}

#[test]
fn the_note_on_a_cut_line_is_not_searched() {
    let ctx = egui::Context::default();
    let max = crate::repo::highlight::MAX_LINE_CHARS;
    let lines = [format!("{} … 1000 more characters", "a".repeat(max))];
    let mut find = Find::default();
    find.show();
    find.query = query("more");
    find.update(
        &ctx,
        file_key("a", 1, 1),
        1,
        |i| super::nth(&lines, i),
        super::Layout::Lines,
    );
    assert_eq!(find.count(), 0);
    find.query = query("a");
    find.update(
        &ctx,
        file_key("a", 1, 1),
        1,
        |i| super::nth(&lines, i),
        super::Layout::Lines,
    );
    assert_eq!(find.count(), max);
}

#[test]
fn tabs_count_four_columns_when_scrolling_sideways() {
    assert_eq!(super::columns("\t\tab", 3), 9);
    assert_eq!(super::columns("äb", 2), 2);
    let mut find = Find::default();
    find.saw(Seen {
        y: 0.0,
        height: 190.0,
        text_w: 70.0,
        line_h: 19.0,
        x: 0.0,
    });
    let hit = super::Hit {
        row: 0,
        side: 0,
        start: 3,
        end: 4,
        line: 0,
    };
    // Three characters in: in view without tabs, 12 columns (84 points) in with them.
    assert_eq!(find.offsets_for(&hit, "abcd", 7.0), (None, None));
    let (_, x) = find.offsets_for(&hit, "\t\t\td", 7.0);
    assert_eq!(x, Some(84.0 - 70.0 / 3.0));
}
