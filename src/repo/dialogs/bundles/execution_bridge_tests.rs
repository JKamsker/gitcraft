use super::*;
use crate::parallel_bisect::{bundle, close_finished};
fn form() -> Box<Form> {
    let super::super::Dialog::BundleIncoming(form) = super::super::Dialog::bundle_incoming(
        "/supplied/repo".into(),
        Default::default(),
        "supplied".into(),
        BundlePreview {
            refs: vec![],
            prerequisites: vec![],
            verification_error: None,
            bytes: std::sync::Arc::new(vec![]),
            digest: "opaque".into(),
            repository: "opaque".into(),
        },
    ) else {
        unreachable!()
    };
    form
}
#[test]
fn execution_bridge_bundle_captures_real_aggregates_through_event_form_and_paint()
-> Result<(), String> {
    let primary = "Contained Git failed (1): provider; quarantine cleanup failed: provider detail";
    let cleanup = "cleanup; quarantine cleanup failed: repeated); $HOME {raw}\nunknown";
    let mut receipt = None;
    let mut cases = Vec::new();
    let raw = bundle::completed_with_presentation(
        Err(primary.into()),
        Err(cleanup.into()),
        false,
        &mut receipt,
        None,
    )
    .err()
    .ok_or("aggregate")?;
    cases.push((
        raw,
        receipt.take(),
        "die Quarantäne konnte nicht bereinigt werden",
    ));
    let keep = bundle::keep_retained(primary.into(), "opaque-hash; pack-user.keep", &mut receipt);
    let raw = bundle::completed_with_presentation(Err(keep), Ok(()), true, &mut receipt, None)
        .err()
        .ok_or("keep")?;
    cases.push((raw, receipt.take(), "die eigene Schutzmarkierung"));
    let lease = close_finished(Err(primary.into()), Err(cleanup.into()), &mut receipt)
        .err()
        .ok_or("lease")?;
    for import in [false, true] {
        let raw = bundle::completed_with_presentation(
            Ok(()),
            Err(lease.clone()),
            import,
            &mut None,
            receipt.clone(),
        )
        .err()
        .ok_or("completion")?;
        let mut output = None;
        let result = bundle::completed_with_presentation(
            Ok(()),
            Err(lease.clone()),
            import,
            &mut output,
            receipt.clone(),
        );
        assert_eq!(result, Err(raw.clone()));
        cases.push((raw, output, "Freigabe der Besitzreservierung"));
    }
    // An outer aggregate replaces the keep receipt and keeps its primary opaque.
    let keep = bundle::keep_retained(primary.into(), "hash", &mut receipt);
    let raw = bundle::completed_with_presentation(
        Err(keep.clone()),
        Err(cleanup.into()),
        true,
        &mut receipt,
        None,
    )
    .err()
    .ok_or("outer")?;
    cases.push((
        raw,
        receipt.take(),
        "die Quarantäne konnte nicht bereinigt werden",
    ));
    for (raw, receipt, german) in cases {
        let mut form = form();
        form.tx
            .send(Event::DoneWithPresentation(
                false,
                Err(raw.clone()),
                receipt.clone(),
            ))
            .map_err(|error| format!("{}: {error:?}", "inert channel"))?;
        form.receive();
        assert_eq!(form.error.as_deref(), Some(raw.as_str()));
        let mut paints = Vec::new();
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            let painted = form.painted_error(form.error.as_deref().ok_or("raw state")?);
            assert!(painted.contains(primary));
            if locale == "de" {
                assert!(painted.contains(german), "{painted}");
            }
            paints.push(painted);
        }
        assert_eq!(paints[0], paints[2]);
        assert_ne!(paints[0], paints[1]);
        form.tx
            .send(Event::DoneWithPresentation(
                false,
                Err("stale unrelated".into()),
                receipt,
            ))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.receive();
        assert!(form.error_presentation.is_none());
        form.tx
            .send(Event::Done(false, Ok(vec![])))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.receive();
        assert!(form.error.is_none() && form.error_presentation.is_none());
    }
    for unknown in [
        "provider text; workspace cleanup: provider detail",
        "provider; quarantine cleanup failed: provider detail",
        "provider; transfer cleanup: provider detail",
        "provider; lease release: provider detail",
        "provider; owned pack-hash.keep retained",
    ] {
        let mut receipt = None;
        let result = bundle::completed_with_presentation(
            Err(unknown.into()),
            Ok(()),
            false,
            &mut receipt,
            None,
        );
        let mut form = form();
        form.tx
            .send(Event::DoneWithPresentation(
                false,
                result.map(|()| vec![]),
                receipt,
            ))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.receive();
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            assert_eq!(
                form.painted_error(form.error.as_deref().ok_or("raw")?),
                unknown
            );
        }
    }

    Ok(())
}
