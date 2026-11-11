use super::*;

#[test]
fn ref_labels_read_for_a_person() {
    assert_eq!(
        ref_label("main~2", RefKind::Branch),
        Some("main".to_owned())
    );
    assert_eq!(
        ref_label("remotes/origin/feat/x^2~1", RefKind::Remote),
        Some("origin/feat/x".to_owned())
    );
    assert_eq!(
        ref_label("tags/v1.0^0", RefKind::Tag),
        Some("tag v1.0".to_owned())
    );
    assert_eq!(
        ref_label("tags/x~1", RefKind::Branch),
        Some("tags/x".to_owned())
    );
    assert_eq!(ref_label("undefined", RefKind::Tag), None);
    assert_eq!(ref_label("", RefKind::Branch), None);
}

#[test]
fn other_refs_leave_out_only_the_moving_ones() {
    let moving = [
        "refs/heads/feat/x".to_owned(),
        "refs/remotes/origin/feat/x".to_owned(),
        "HEAD".to_owned(),
    ];
    assert_eq!(
        other_refs(&moving),
        [
            "--not",
            "--exclude=feat/x",
            "--branches",
            "--exclude=origin/feat/x",
            "--exclude=*/HEAD",
            "--remotes",
            "--tags",
            "--not",
        ]
    );
}

#[test]
fn a_range_without_keep_is_the_tip_alone() {
    assert_eq!(Range::new("abc", Some("")).revisions(), ["abc"]);
    assert_eq!(Range::new("abc", None).revisions(), ["abc"]);
    assert_eq!(Range::new("abc", Some("def")).revisions(), ["abc", "^def"]);
}

#[test]
fn name_rev_lines_parse() {
    let out = "aaa main~1\nbbb undefined\nccc remotes/origin/main\n";
    assert_eq!(
        parse_names(out, RefKind::Remote),
        [
            ("aaa".to_owned(), Some("main".to_owned())),
            ("bbb".to_owned(), None),
            ("ccc".to_owned(), Some("origin/main".to_owned())),
        ]
    );
}

#[test]
fn asks_name_what_moves() {
    let reset = PreviewAsk::reset("HEAD", "h", "t", None, false);
    assert_eq!(reset.moving, ["HEAD"]);
    assert_eq!(reset.files, LostFiles::None);
    let hard = PreviewAsk::reset(
        "main",
        "h",
        "t",
        Some("refs/remotes/origin/main".into()),
        true,
    );
    assert_eq!(hard.moving, ["refs/heads/main"]);
    assert_eq!(hard.files, LostFiles::HardReset(Some("t".to_owned())));
    assert_eq!(hard.rewritten, Some(Range::new("h", Some("t"))));
    // HEAD unborn: everything on the branch leaves.
    let delete = PreviewAsk::delete(&["tip"], "", vec!["refs/heads/x".into()]);
    assert_eq!(delete.leaving, Some(Range::new("tip", None)));
    let dropped = PreviewAsk::drop_commit("c", "p", "main", "h", None);
    assert_eq!(dropped.leaving, Some(Range::new("c", Some("p"))));
    assert_eq!(dropped.rewritten, Some(Range::new("h", Some("p"))));
}

#[test]
fn only_git_work_reads_git() {
    let mut ask = PreviewAsk::nothing();
    assert!(!ask.reads_git());
    ask.files = LostFiles::Listed(vec!["a".to_owned()]);
    assert!(!ask.reads_git());
    ask.files = LostFiles::HardReset(None);
    assert!(ask.reads_git());
}
