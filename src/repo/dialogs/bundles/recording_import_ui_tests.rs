use super::*;
#[test]
fn confirmation_is_explicit_and_late_owned_cleanup_errors_survive_stop() -> Result<(), String> {
    let oid = "a".repeat(40);
    let bytes = format!("# v2 git bundle\n{oid} refs/heads/main\n\nPACK").into_bytes();
    let preview = BundlePreview {
        refs: vec![BundleRef {
            name: "refs/heads/main".into(),
            oid,
        }],
        prerequisites: vec![],
        verification_error: None,
        digest: crate::git::bundle_digest(&bytes),
        bytes: std::sync::Arc::new(bytes),
        repository: "/recording/common".into(),
    };
    let dialog = super::super::Dialog::bundle_incoming(
        "/recording/repo".into(),
        crate::tasks::Cancellation::default(),
        "held.bundle".into(),
        preview,
    );
    let super::super::Dialog::BundleIncoming(mut form) = dialog else {
        return Err("wrong dialog".into());
    };
    assert!(!form.consent && !form.confirm && !form.verified && form.active.is_none());
    let token = form.scope.0.child();
    token.cancel();
    form.active = Some(token);
    form.tx
        .send(Event::Done(
            true,
            Err("cancelled; owned cleanup unconfirmed; ledger retained".into()),
        ))
        .map_err(|error| error.to_string())?;
    form.receive();
    assert!(
        form.error
            .as_deref()
            .is_some_and(|error| error.contains("owned cleanup unconfirmed"))
    );
    assert!(form.active.is_none() && form.imported.is_empty() && !form.confirm);
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 1400.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        assert!(matches!(form.show(ui), Outcome::Open));
    });
    output.textures_delta.clear();
    assert!(form.active.is_none(), "rendering must not submit a job");
    assert!(!form.consent && !form.confirm && !form.verified);
    Ok(())
}
