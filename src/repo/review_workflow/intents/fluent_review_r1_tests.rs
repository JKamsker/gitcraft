use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_r1_actual_quarantine_header_retains_expansion_across_locale_and_count()
-> Result<(), String> {
    let _pause = network::test_pause::set(None);
    let ctx = egui::Context::default();
    let mut state = State {
        quarantine: vec!["opaque quarantine record {raw}".into()],
        generation: 7,
        ..Default::default()
    };
    let mut acts = vec![];
    crate::i18n::set_language("en-US");
    let text = paint(&ctx, |ui| show(ui, &mut state, &[], &mut acts));
    assert!(!text.contains("opaque quarantine record {raw}"));
    click(
        &ctx,
        "One invalid/oversize intent record quarantined",
        |ui| show(ui, &mut state, &[], &mut acts),
    )?;
    for (locale, heading) in [
        ("en-US", "One invalid/oversize intent record quarantined"),
        ("de", "Ein ungültiger/zu großer Absichtseintrag isoliert"),
        ("en-US", "One invalid/oversize intent record quarantined"),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| show(ui, &mut state, &[], &mut acts));
        assert!(
            text.contains(heading) && text.contains("opaque quarantine record {raw}"),
            "quarantine expansion lost: {text}"
        );
    }
    state.quarantine.push("second opaque record --argv".into());
    crate::i18n::set_language("de");
    let text = paint(&ctx, |ui| show(ui, &mut state, &[], &mut acts));
    assert!(
        text.contains("2 ungültige/zu große Absichtseinträge isoliert")
            && text.contains("second opaque record --argv"),
        "count change lost expansion: {text}"
    );
    click(
        &ctx,
        "2 ungültige/zu große Absichtseinträge isoliert",
        |ui| show(ui, &mut state, &[], &mut acts),
    )?;
    crate::i18n::set_language("en-US");
    let text = paint(&ctx, |ui| show(ui, &mut state, &[], &mut acts));
    assert!(
        !text.contains("second opaque record --argv"),
        "closed state not retained: {text}"
    );
    assert_eq!(state.generation, 7);
    assert!(acts.is_empty());
    Ok(())
}
