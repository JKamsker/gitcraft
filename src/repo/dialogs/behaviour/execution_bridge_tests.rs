use super::*;
use crate::parallel_bisect::close_finished;
#[test]
fn execution_bridge_observation_reaches_both_failed_states_and_repaints() -> Result<(), String> {
    let primary = "Observation checkout failed: provider; workspace cleanup: provider detail";
    let cleanup = "cleanup); recovery evidence was retained. $HOME; workspace cleanup: repeated";
    let mut cases = Vec::new();
    let mut receipt = None;
    let result = observe::finish_with_presentation(
        Err(primary.into()),
        Err(cleanup.into()),
        &mut receipt,
        None,
    );
    cases.push((result, receipt.take(), "Bereinigung des Arbeitsbereichs"));
    let lease = close_finished(Err(primary.into()), Err(cleanup.into()), &mut receipt)
        .err()
        .ok_or("lease")?;
    let result = observe::finish_with_presentation(
        Ok(Observation::Inconsistent),
        Err(lease),
        &mut None,
        receipt.clone(),
    );
    cases.push((result, receipt.take(), "Freigabe der Besitzreservierung"));
    for (result, receipt, german) in cases {
        let raw = result.as_ref().err().ok_or("raw error")?.clone();
        let mut form = Form::new(
            "/supplied/repo".into(),
            Default::default(),
            Default::default(),
            "HEAD".into(),
        );
        form.frames = vec![Frame::Queued];
        form.tx
            .send(Event::FrameWithPresentation(0, result, receipt.clone()))
            .map_err(|error| format!("{}: {error:?}", "inert channel"))?;
        form.tx
            .send(Event::DoneWithPresentation(
                Err(raw.clone()),
                receipt.clone(),
            ))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.poll();
        assert_eq!(form.error.as_deref(), Some(raw.as_str()));
        let Frame::FailedWithPresentation(frame_raw, frame_receipt) = &form.frames[0] else {
            return Err("receipt lost".into());
        };
        assert_eq!(frame_raw, &raw);
        let mut paints = Vec::new();
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            let paint = frame_error(frame_raw, Some(frame_receipt));
            assert_eq!(
                paint,
                form.painted_error(form.error.as_deref().ok_or("raw")?)
            );
            assert!(paint.contains(primary) && paint.contains(cleanup));
            if locale == "de" {
                assert!(paint.contains(german));
            }
            paints.push(paint);
        }
        assert_eq!(paints[0], paints[2]);
        assert_ne!(paints[0], paints[1]);
        form.tx
            .send(Event::FrameWithPresentation(
                0,
                Err("stale".into()),
                receipt.clone(),
            ))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.tx
            .send(Event::DoneWithPresentation(Err("stale".into()), receipt))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.poll();
        assert!(matches!(&form.frames[0], Frame::Failed(raw) if raw == "stale"));
        assert!(form.error_presentation.is_none());
        form.tx
            .send(Event::FrameWithPresentation(
                0,
                Ok(Observation::Inconsistent),
                None,
            ))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.tx
            .send(Event::Done(Ok(())))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.poll();
        assert!(matches!(form.frames[0], Frame::Observed(_)));
        assert!(form.error_presentation.is_none());
    }
    for unknown in [
        "provider text; workspace cleanup: provider detail",
        "provider; owned pack-hash.keep retained",
    ] {
        let mut receipt = None;
        let result =
            observe::finish_with_presentation(Err(unknown.into()), Ok(()), &mut receipt, None);
        let mut form = Form::new(
            "/supplied".into(),
            Default::default(),
            Default::default(),
            "HEAD".into(),
        );
        form.frames = vec![Frame::Queued];
        form.tx
            .send(Event::FrameWithPresentation(0, result, receipt))
            .map_err(|error| format!("{}: {error:?}", "channel"))?;
        form.poll();
        let Frame::Failed(raw) = &form.frames[0] else {
            return Err("unknown acquired provenance".into());
        };
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            assert_eq!(frame_error(raw, None), unknown);
        }
    }

    Ok(())
}
