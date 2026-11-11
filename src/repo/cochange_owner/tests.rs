use super::*;
fn locate(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<egui::Pos2> {
    fn visit(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.job.text == needle => {
                Some(t.pos + t.galley.size() / 2.0)
            }
            egui::Shape::Vec(v) => v.iter().find_map(|s| visit(s, needle)),
            _ => None,
        }
    }
    shapes.iter().find_map(|s| visit(&s.shape, needle))
}
#[test]
fn painted_hint_can_be_dismissed_restored_and_open_its_commit_evidence() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/recording-only/cochange-ui".into(), &ctx);
    tab.work.staged.push(crate::git::StatusFile {
        path: "serializer.rs".into(),
        old_path: None,
        code: 'M',
        area: crate::git::Area::Staged,
    });
    let hash = "a".repeat(40);
    tab.cochanges.hints.push(Hint {
        source: "serializer.rs".into(),
        companion: "compat.fixture".into(),
        source_changes: 3,
        evidence: vec![(hash.clone(), "Change serializer and fixture".into()); 3],
    });
    let mut time = 0.;
    let mut acts = Vec::new();
    let mut frame = |tab: &mut RepoTab, events: Vec<egui::Event>, acts: &mut Vec<Action>| {
        time += 0.05;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 900.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| tab.cochange_hints(ui, acts));
            },
        );
        output.textures_delta.clear();
        output.shapes
    };
    let mut click =
        |tab: &mut RepoTab, label: &str, acts: &mut Vec<Action>| -> Result<(), String> {
            let mut pos = None;
            for _ in 0..8 {
                pos = locate(&frame(tab, vec![], acts), label);
                if pos.is_some() {
                    break;
                }
            }
            let pos = pos.ok_or_else(|| format!("Missing visible label {label}"))?;
            for pressed in [true, false] {
                frame(
                    tab,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    acts,
                );
            }
            Ok(())
        };
    click(&mut tab, "Dismiss", &mut acts)?;
    assert_eq!(tab.cochanges.dismissed.len(), 1);
    click(&mut tab, "Restore dismissed hints", &mut acts)?;
    assert!(tab.cochanges.dismissed.is_empty());
    click(&mut tab, "Evidence: 3 shared commits", &mut acts)?;
    click(
        &mut tab,
        &format!(
            "{} · Change serializer and fixture",
            super::super::short(&hash)
        ),
        &mut acts,
    )?;
    assert!(
        acts.iter()
            .any(|a| matches!(a,Action::SelectHash(id) if id==&hash))
    );
    Ok(())
}

#[test]
fn context_retirement_rejects_reads_and_preserves_mutation_delivery() -> Result<(), String> {
    use crate::repo::{
        OpKind, Refresh, Undo,
        messages::Msg,
        timeline::{Awaiting, Ended},
    };
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/context-retirement".into(), &ctx);
    let head = "a".repeat(40);
    let Effect::Read { request, .. } = tab.cochanges.context(&tab.path, &head) else {
        return Err("expected a pending cochange read".into());
    };
    // A connection revision changes even when the final snapshot returns to A.
    tab.connection_generation = crate::connections::generation().wrapping_add(1);
    tab.notice_connections(&ctx);
    tab.cochanges.complete(Completion {
        request,
        result: Err("stale A".into()),
    });
    assert!(tab.cochanges.error.is_none());
    let Effect::Read { request, .. } = tab.cochanges.context(&tab.path, &head) else {
        return Err("expected a pending cochange read before sign-in".into());
    };
    tab.set_credentials(crate::git::Credentials {
        username: "recording-user".into(),
        password: "recording-only".into(),
        scope: None,
    });
    tab.cochanges.complete(Completion {
        request,
        result: Err("stale sign-in context".into()),
    });
    assert!(tab.cochanges.error.is_none());
    let old = std::time::Instant::now() - crate::repo::lifecycle::IDLE_AFTER;
    tab.life.shown(old);
    tab.busy_op = Some("Synthetic mutation".into());
    let sender = tab.tx.clone();
    // Explicit A→B→A hooks retire reads without resetting executing accounting.
    tab.invalidate_repository_context();
    tab.invalidate_repository_context();
    tab.background_tick();
    assert!(
        !tab.sleeping(),
        "an executing mutation retains its receiver"
    );
    assert_eq!(tab.busy_op.as_deref(), Some("Synthetic mutation"));
    sender
        .send(Msg::Then(Box::new(|tab, ctx| {
            tab.op_done(
                "Synthetic mutation".into(),
                Ok(String::new()),
                Refresh::All,
                OpKind::Normal(
                    Undo::Custom {
                        undo: crate::repo::ops(&[&["branch", "-D", "created"]]),
                        redo: crate::repo::ops(&[&["branch", "created", "tip"]]),
                    },
                    String::new(),
                ),
                false,
                ctx,
            );
        })))
        .map_err(|_| "mutation receiver disconnected")?;
    let message = tab.rx.try_recv().map_err(|error| error.to_string())?;
    crate::repo::messages::record_jobs(|| tab.handle(message, &ctx));
    assert!(tab.busy_op.is_none());
    assert!(matches!(
        tab.timeline.awaiting,
        Awaiting::Ended {
            then: Ended::New { undo: Some(_), .. },
            ..
        }
    ));
    Ok(())
}
