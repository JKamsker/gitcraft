use super::*;

const BEFORE: &str = "\
fn main() {
<<<<<<< HEAD
    let names = [\"ada\", \"grace\"];
=======
    let names = [\"ada\"];
>>>>>>> greeting
    for n in names {
<<<<<<< HEAD
        println!(\"hey {n}\");
=======
        println!(\"hello, {n}!\");
>>>>>>> greeting
    }
}
";

const AFTER: &str = "\
fn main() {
    let names = [\"ada\", \"grace\"];
    for n in names {
        println!(\"hello, {n}!\");
    }
}
";

fn conflicts(review: &FileReview) -> (&[Resolved], bool) {
    match review {
        FileReview::Conflicts {
            resolved,
            changed_outside,
            ..
        } => (resolved, *changed_outside),
        // Not lined up: no conflicts, which the callers' asserts catch.
        _ => (&[], false),
    }
}

#[test]
fn each_conflict_is_shown_with_its_sides_and_what_the_agent_wrote() {
    let review = review(Some(BEFORE), Some(AFTER));
    let (resolved, outside) = conflicts(&review);
    assert!(!outside);
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].took, Took::Ours);
    assert_eq!(resolved[0].line, 2);
    assert_eq!(resolved[1].took, Took::Theirs);
    assert_eq!(resolved[1].result, "        println!(\"hello, {n}!\");\n");
    assert_eq!(resolved[1].line, 4);
    if let FileReview::Conflicts {
        ours_label,
        theirs_label,
        ..
    } = &review
    {
        assert_eq!(
            (ours_label.as_str(), theirs_label.as_str()),
            ("HEAD", "greeting")
        );
    }
}

#[test]
fn what_the_agent_kept_is_named() {
    let (o, t) = ("a\n", "b\n");
    assert_eq!(Took::of(o, t, "a\nb\n"), Took::Both);
    assert_eq!(Took::of(o, t, "b\na\n"), Took::BothSwapped);
    assert_eq!(Took::of(o, t, ""), Took::Removed);
    assert_eq!(Took::of(o, t, "c\n"), Took::Rewrote);
    assert_eq!(Took::Both.words("main", "dev"), "kept both, main first");
}

#[test]
fn a_change_outside_the_conflicts_is_flagged_and_in_the_diff() {
    let after = AFTER.replace("fn main() {", "fn main() -> () {");
    let review = review(Some(BEFORE), Some(&after));
    let (resolved, outside) = conflicts(&review);
    assert!(outside, "the first line changed");
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[1].took, Took::Theirs);
    let FileReview::Conflicts { diff, .. } = &review else {
        return;
    };
    assert!(diff.contains(&Line::Added("fn main() -> () {".into())));
    assert!(diff.contains(&Line::Removed("fn main() {".into())));
}

#[test]
fn text_that_cant_be_lined_up_is_shown_as_a_diff() {
    let review = review(Some(BEFORE), Some("something else entirely\n"));
    assert!(matches!(review, FileReview::Diff(_)), "{review:?}");
}

#[test]
fn deleted_made_and_markerless_files_say_so() {
    assert!(matches!(
        review(Some("x\n"), None),
        FileReview::Deleted { .. }
    ));
    assert!(matches!(
        review(None, Some("x\n")),
        FileReview::Created { .. }
    ));
    // A file without markers is a diff.
    assert_eq!(
        review(Some("a\nb\n"), Some("a\nc\n")),
        FileReview::Diff(vec![
            Line::Same("a".into()),
            Line::Removed("b".into()),
            Line::Added("c".into())
        ])
    );
}

#[test]
fn a_diff_keeps_a_few_lines_around_each_change() {
    let before: String = (0..20).map(|i| format!("{i}\n")).collect();
    let after = before.replace("10\n", "ten\n");
    let diff = line_diff(&before, &after);
    assert_eq!(diff.first(), Some(&Line::Skipped(7)));
    assert_eq!(diff.last(), Some(&Line::Skipped(6)));
    assert!(diff.contains(&Line::Removed("10".into())));
    assert!(diff.contains(&Line::Added("ten".into())));
    assert_eq!(
        diff.iter().filter(|l| matches!(l, Line::Same(_))).count(),
        6
    );
}

#[test]
fn a_conflict_at_the_end_of_the_file_is_lined_up_too() {
    let before = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\n";
    let review = review(Some(before), Some("a\nx\ny\n"));
    let (resolved, outside) = conflicts(&review);
    assert!(!outside);
    assert_eq!(resolved[0].took, Took::Both);
}
