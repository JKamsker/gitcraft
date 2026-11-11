use super::*;

#[test]
fn completed_failure_receipt_survives_status_queue_and_later_event() -> Result<(), String> {
    let primary = "provider; transfer cleanup: owned-looking detail";
    let cleanup = "native cleanup; lease release: opaque $(keep)";
    let mut receipt = None;
    let raw = pb::supplied_transfer(primary, Some(cleanup), &mut receipt)
        .err()
        .ok_or("expected supplied failure")?;
    let mut paint = PresentationState::default();
    let Event::Done(outcome) = paint.take(pb::done_with_presentation(
        Outcome::Failed(raw.clone()),
        receipt,
    )) else {
        return Err("missing supplied completion".into());
    };
    let status = completion_notice(&outcome, &Machine::Here, &paint);
    let original = outcome_message(&outcome, &Machine::Here.label());
    assert_eq!(status.raw(), &original);
    let name = "repo · : $(opaque)";
    let finished = status.clone().for_repository(name);
    let queued_raw = (format!("{name} · {}", original.0), original.1);
    assert_eq!(finished.raw(), &queued_raw);
    paint.take(Event::Done(Outcome::Cancelled));
    assert!(paint.failed.is_none());
    let mut renders = Vec::new();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        let displayed = status.projected().ok_or("lost status receipt")?;
        let queued = finished.projected().ok_or("lost queued receipt")?;
        assert!(displayed.contains(primary) && displayed.contains(cleanup));
        assert_eq!(queued, format!("{name} · {displayed}"));
        if locale == "de" {
            assert!(displayed.starts_with("Parallele Bisektion fehlgeschlagen:"));
            assert!(displayed.contains("Bereinigung der Übertragung"));
        }
        assert_eq!(status.raw(), &original);
        assert_eq!(finished.raw(), &queued_raw);
        renders.push(displayed);
    }
    assert_eq!(renders[0], renders[2]);
    assert_ne!(renders[0], renders[1]);
    let plain = crate::repo::Notice::plain(original);
    assert_eq!(plain.projected(), None);
    Ok(())
}

#[test]
fn all_outcomes_preserve_raw_flags_and_only_source_fields_are_displayed() -> Result<(), String> {
    let hash = "a".repeat(40);
    let sentinel = "test output must never be retained in a completion notice";
    let cases = [
        Outcome::FirstBad(hash.clone()),
        Outcome::OneOf(vec![hash.clone(); 1234]),
        Outcome::GoodEndFails {
            hash: hash.clone(),
            output: sentinel.into(),
        },
        Outcome::BadEndPasses {
            hash,
            output: sentinel.into(),
        },
        Outcome::Cancelled,
        Outcome::Failed(
            "opaque child; transfer cleanup: unowned sentence\nsecond opaque line".into(),
        ),
    ];
    for outcome in cases {
        let source = completion_notice(&outcome, &Machine::Here, &PresentationState::default());
        let raw = outcome_message(&outcome, &Machine::Here.label());
        assert_eq!(source.raw(), &raw);
        assert!(!format!("{source:?}").contains(sentinel));
        let mut renders = Vec::new();
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            let displayed = source.projected().ok_or("missing supplied projection")?;
            if matches!(outcome, Outcome::Failed(_)) {
                assert!(displayed.contains("opaque child; transfer cleanup: unowned sentence"));
                assert!(!displayed.contains("second opaque line"));
            }
            if matches!(outcome, Outcome::OneOf(_)) {
                assert!(displayed.contains(if locale == "de" { "1.234" } else { "1,234" }));
            }
            assert_eq!(source.raw(), &raw);
            renders.push(displayed);
        }
        assert_eq!(renders[0], renders[2]);
        assert_ne!(renders[0], renders[1]);
    }
    Ok(())
}
