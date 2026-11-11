use super::*;
fn form() -> Result<Form, Box<dyn std::error::Error>> {
    let mut form = Form::new(
        "/synthetic/repo".into(),
        Default::default(),
        Default::default(),
        "HEAD".into(),
    );
    let commits = (0..4)
        .map(|i| git::BehaviourCommit {
            hash: format!("{i:040x}"),
            tree: format!("{:040x}", i + 1),
            summary: format!("frame {i}"),
        })
        .collect();
    form.tx.send(Event::Loaded(commits))?;
    form.poll();
    Ok(form)
}
#[test]
fn recording_timeline_dialog_scrubs_injected_stable_unknown_and_inconsistent_frames()
-> Result<(), Box<dyn std::error::Error>> {
    let mut form = form()?;
    assert!(form.frames.iter().all(|f| matches!(f, Frame::Unknown)));
    form.cache.put(
        form.key(&form.commits[0].tree),
        Observation::Stable(Value::Text("frame output".into()).into()),
    );
    form.cache.put(
        form.key(&form.commits[1].tree),
        Observation::Stable(Value::Text("changed".into()).into()),
    );
    form.cache
        .put(form.key(&form.commits[2].tree), Observation::Inconsistent);
    form.refresh_cache();
    assert_eq!(form.first_change, Some(1));
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            assert!(matches!(form.show(ui), Outcome::Open));
        },
    );
    output.textures_delta.clear();
    let text: String = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("frame output"));
    assert!(text.contains("inconsistent repeats"));
    assert!(text.contains("not run / evicted"));
    assert!(text.contains("Open first observed adjacent change"));
    assert!(form.active.is_none());
    Ok(())
}
#[test]
fn recording_late_worker_error_survives_cancelled_token_and_completion()
-> Result<(), Box<dyn std::error::Error>> {
    let mut form = form()?;
    let token = form.scope.0.child();
    token.cancel();
    form.active = Some(token);
    form.frames[0] = Frame::Queued;
    form.tx
        .send(Event::Frame(0, Err("late concrete owner error".into())))?;
    form.tx
        .send(Event::Done(Err("workspace cleanup unknown".into())))?;
    form.poll();
    assert!(matches!(&form.frames[0], Frame::Failed(e) if e == "late concrete owner error"));
    assert_eq!(form.error.as_deref(), Some("workspace cleanup unknown"));
    assert!(form.active.is_none());
    Ok(())
}
