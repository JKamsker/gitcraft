//! The coverage chip and marks in words: a report of another revision is conspicuous, and a
//! mark's name tells unknown from uncovered.
use super::*;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn gutter(trust: Trust) -> Gutter {
    Gutter {
        report: "lcov.info".into(),
        sha: SHA.into(),
        trust,
        marks: HashMap::new(),
    }
}

#[test]
fn a_report_of_another_revision_gets_a_conspicuous_stale_chip() {
    let (text, tip, warn) = banner_text(&gutter(Trust::Other("fedcba9876543210".into())));
    assert!(warn);
    assert!(
        text.contains("fedcba98") && text.contains("stale"),
        "{text}"
    );
    assert!(
        tip.contains("01234567") && tip.contains("lcov.info.sha"),
        "{tip}"
    );
    let (text, _, warn) = banner_text(&gutter(Trust::Current));
    assert!(!warn);
    assert!(
        text.contains("lcov.info") && text.contains("01234567"),
        "{text}"
    );
    for trust in [
        Trust::EditedAfter,
        Trust::Undetermined,
        Trust::IndexDiffers,
        Trust::NoRevision,
    ] {
        let (text, _, warn) = banner_text(&gutter(trust.clone()));
        assert!(warn, "{trust:?}");
        assert!(
            text.contains("not applied") || text.contains("stale"),
            "{text}"
        );
    }
}

#[test]
fn the_stale_chip_is_said_in_every_language() {
    let g = gutter(Trust::Other("fedcba9876543210".into()));
    crate::i18n::set_language("de");
    let (german, _, _) = banner_text(&g);
    let unknown = mark_text(Mark::Unknown);
    crate::i18n::set_language("en-US");
    let (english, _, _) = banner_text(&g);
    assert_ne!(german, english);
    assert!(
        german.contains("veraltet") && german.contains("fedcba98"),
        "{german}"
    );
    assert_ne!(unknown, mark_text(Mark::Unknown));
    // Unknown never reads as uncovered.
    assert_ne!(mark_text(Mark::Unknown), mark_text(Mark::Uncovered));
    assert!(mark_text(Mark::Covered(1)).contains("1 time"));
    assert!(mark_text(Mark::Covered(3)).contains("3 times"));
}

#[test]
fn only_the_new_side_of_added_lines_gets_a_mark() {
    let line = |kind, new| super::super::DiffLine {
        kind,
        old: None,
        new,
        text: String::new(),
        raw: String::new(),
    };
    let mut data = DiffData::default();
    data.lines = vec![
        line(Kind::Context, Some(1)),
        line(Kind::Add, Some(2)),
        line(Kind::Del, None),
    ];
    let mut g = gutter(Trust::Current);
    g.marks = HashMap::from([(1, Mark::Covered(1)), (2, Mark::Uncovered)]);
    assert_eq!(mark_of(&g, &data, 0), None);
    assert_eq!(mark_of(&g, &data, 1), Some(Mark::Uncovered));
    assert_eq!(mark_of(&g, &data, 2), None);
    let off = Overlay {
        asked: Some(0),
        gutter: Some(Ok(Some(Arc::new(g)))),
    };
    assert!(off.shown(true).is_some());
    assert!(off.shown(false).is_none());
}
