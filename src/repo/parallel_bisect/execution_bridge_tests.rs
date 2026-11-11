use super::*;
#[test]
fn execution_bridge_transfer_and_cleanup_warning_use_real_events_and_banner_state()
-> Result<(), String> {
    let primary = "provider; transfer cleanup: provider detail; lease release: inner";
    let cleanup =
        "cleanup); recovery evidence was retained. (repeated); transfer cleanup: $HOME {raw}";
    let mut receipt = None;
    let raw = pb::supplied_transfer(primary, Some(cleanup), &mut receipt)
        .err()
        .ok_or("transfer")?;
    let mut state = PresentationState::default();
    assert!(
        matches!(state.take(pb::done_with_presentation(Outcome::Failed(raw.clone()), receipt.clone())), Event::Done(Outcome::Failed(ref value)) if value == &raw)
    );
    // Foreign generic caches retain their raw identity. Their typed final-paint
    // integration remains a root-owned seam; this test exercises banner paint.
    let cached_status = outcome_message(&Outcome::Failed(raw.clone()), "opaque machine");
    let mut paints = Vec::new();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(
            outcome_message(&Outcome::Failed(raw.clone()), "opaque machine"),
            cached_status
        );
        let painted = state.error(&raw);
        assert!(painted.contains(primary) && painted.contains(cleanup));
        if locale == "de" {
            assert!(painted.contains("Bereinigung der Übertragung"));
        }
        let (status, failed) = outcome_message(&Outcome::Failed(painted.clone()), "opaque machine");
        assert!(failed && status.contains(&painted));
        paints.push(painted);
    }
    assert_eq!(paints[0], paints[2]);
    assert_ne!(paints[0], paints[1]);
    state.take(pb::done_with_presentation(
        Outcome::Failed("unrelated".into()),
        receipt.clone(),
    ));
    assert!(state.failed.is_none());
    state.take(pb::done_with_presentation(
        Outcome::Cancelled,
        receipt.clone(),
    ));
    assert!(state.failed.is_none());
    for unknown in [
        "provider text; workspace cleanup: provider detail",
        "provider; owned pack-hash.keep retained",
    ] {
        let result = pb::supplied_transfer(unknown, None, &mut receipt);
        let raw = result.err().ok_or("forwarded")?;
        assert!(receipt.is_none());
        state.take(pb::done_with_presentation(
            Outcome::Failed(raw.clone()),
            receipt.clone(),
        ));
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            assert_eq!(state.error(&raw), unknown);
        }
    }
    let lease = pb::close_finished(Err(primary.into()), Err(cleanup.into()), &mut receipt)
        .err()
        .ok_or("lease")?;
    let Event::Warning(raw_warning) = state.take(pb::cleanup_warning(
        "opaque host; )".into(),
        lease.clone(),
        receipt.clone(),
        true,
    )) else {
        return Err("warning".into());
    };
    let mut paints = Vec::new();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        let painted = state.warning(&raw_warning);
        assert!(
            painted.contains(primary)
                && painted.contains(cleanup)
                && painted.contains("opaque host; )")
        );
        if locale == "de" {
            assert!(
                painted.contains("Freigabe der Besitzreservierung")
                    && painted.contains("Wiederherstellungsnachweise")
            );
        }
        paints.push(painted);
    }
    assert_eq!(paints[0], paints[2]);
    assert_ne!(paints[0], paints[1]);
    assert_eq!(state.warning("stale warning"), "stale warning");
    state.take(Event::Warning("unrelated".into()));
    assert!(state.warning.is_none());
    state.take(Event::Done(Outcome::Cancelled));
    assert!(state.failed.is_none());
    let Event::Warning(raw_warning) = state.take(pb::cleanup_warning(
        "opaque host".into(),
        "provider; lease release: unknown".into(),
        None,
        true,
    )) else {
        return Err("warning".into());
    };
    crate::i18n::set_language("de");
    assert!(
        state
            .warning(&raw_warning)
            .contains("provider; lease release: unknown")
    );
    crate::i18n::set_language("en-US");

    Ok(())
}
