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

fn line(kind: Kind, new: Option<u32>) -> super::super::DiffLine {
    super::super::DiffLine {
        kind,
        old: None,
        new,
        text: String::new(),
        raw: String::new(),
    }
}

/// Lines 1 (unchanged), 2 (added) and one removed.
fn data() -> DiffData {
    DiffData {
        lines: vec![
            line(Kind::Context, Some(1)),
            line(Kind::Add, Some(2)),
            line(Kind::Del, None),
        ],
        ..Default::default()
    }
}

fn answer(trust: Trust) -> Result<Option<Arc<Gutter>>, String> {
    let mut g = gutter(trust);
    g.marks = HashMap::from([(1, Mark::Covered(1)), (2, Mark::Uncovered)]);
    Ok(Some(Arc::new(g)))
}

#[test]
fn only_the_new_side_of_added_lines_gets_a_mark() {
    let data = data();
    let mut overlay = Overlay::default();
    let ticket = overlay.begin(7, 1, SHA);
    overlay.accept(ticket, answer(Trust::Current));
    assert_eq!(overlay.mark(&data, 0), None);
    assert_eq!(overlay.mark(&data, 1), Some(Mark::Uncovered));
    assert_eq!(overlay.mark(&data, 2), None);
    // Turned off by any route: forgotten, so it is read afresh when turned on.
    overlay.sync(true);
    assert_eq!(overlay.mark(&data, 1), Some(Mark::Uncovered));
    overlay.sync(false);
    assert_eq!(overlay.mark(&data, 1), None);
    assert_eq!(overlay.asked, None);
}

#[test]
fn only_the_latest_read_is_taken() {
    let data = data();
    let mut overlay = Overlay::default();
    let first = overlay.begin(1, 10, SHA);
    // Off and on again before the first read answered: a second read of the same lines.
    let second = overlay.begin(1, 10, SHA);
    overlay.accept(first, answer(Trust::Current));
    assert_eq!(
        overlay.mark(&data, 1),
        None,
        "an older read's answer is dropped"
    );
    overlay.accept(second, answer(Trust::Current));
    assert_eq!(overlay.mark(&data, 1), Some(Mark::Uncovered));
}

#[test]
fn a_gutter_read_again_is_kept_only_pending_and_only_for_the_same_revision() {
    let data = data();
    let mut overlay = Overlay::default();
    let ticket = overlay.begin(1, 10, SHA);
    overlay.accept(ticket, answer(Trust::Current));
    // A refresh, same lines, same revision: kept, but its marks say unknown until the answer,
    // and the banner says it is being checked.
    let refresh = overlay.begin(2, 10, SHA);
    assert!(overlay.pending);
    assert_eq!(overlay.mark(&data, 1), Some(Mark::Unknown));
    overlay.accept(refresh, answer(Trust::EditedAfter));
    assert!(!overlay.pending);
    assert_eq!(overlay.mark(&data, 1), Some(Mark::Uncovered));
    // HEAD moved (or a commit was made): the old verdict isn't kept at all.
    let _moved = overlay.begin(3, 10, "fedcba9876543210fedcba9876543210fedcba98");
    assert_eq!(overlay.mark(&data, 1), None);
    assert!(!overlay.pending);
    // Nor for other lines, nor when the tab doesn't know the revision.
    let again = overlay.begin(4, 10, SHA);
    overlay.accept(again, answer(Trust::Current));
    let _other = overlay.begin(5, 11, SHA);
    assert_eq!(overlay.mark(&data, 1), None);
    let again = overlay.begin(6, 10, SHA);
    overlay.accept(again, answer(Trust::Current));
    let _unknown = overlay.begin(7, 10, "");
    assert_eq!(overlay.mark(&data, 1), None);
}
