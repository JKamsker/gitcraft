use super::*;

fn step(short: &str, summary: &str) -> Step {
    Step {
        hash: format!("{short}000"),
        short: short.to_owned(),
        author: "a".to_owned(),
        time: 0,
        summary: summary.to_owned(),
    }
}

#[test]
fn the_answer_names_the_pair_and_its_sides() {
    let mut timelines = Timelines {
        ours: vec![step("a1", "add caching"), step("a2", "tune")],
        theirs: vec![step("b1", "change ownership"), step("b2", "")],
        truncated: (false, false),
        base: None,
        theirs_rev: None,
    };
    let names = ("Ours (main)", "Theirs (x)");
    let pair = |ours, theirs, checked| Found::Pair {
        ours,
        theirs,
        checked,
    };
    let text = answer_text(&pair(0, 0, true), &timelines, names);
    assert!(text.contains("a1 “add caching” (Ours (main))"), "{text}");
    assert!(
        text.contains("b1 “change ownership” (Theirs (x))"),
        "{text}"
    );
    assert!(text.contains("just before each don't"), "{text}");
    let unchecked = answer_text(&pair(1, 1, false), &timelines, names);
    assert!(unchecked.contains("b2 (Theirs"), "{unchecked}");
    assert!(
        unchecked.contains("earlier pair may conflict"),
        "{unchecked}"
    );
    timelines.truncated = (true, false);
    assert!(answer_text(&pair(0, 1, true), &timelines, names).contains("started earlier"));
    assert!(!answer_text(&pair(1, 1, true), &timelines, names).contains("started earlier"));
    let gone = answer_text(&pair(5, 0, true), &timelines, names);
    assert!(gone.contains("changed meanwhile"));
    assert!(answer_text(&Found::OneSided, &timelines, names).contains("Only one side"));
    assert!(answer_text(&Found::TipsMergeCleanly, &timelines, names).contains("latest commits"));
}

#[test]
fn the_merge_count_reads_well() {
    assert_eq!(merges_text(0), "Testing merges…");
    assert_eq!(merges_text(1), "1 test merge so far…");
    assert_eq!(merges_text(4), "4 test merges so far…");
}
