use super::*;
fn locate(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<egui::Pos2> {
    fn visit(s: &egui::Shape, n: &str) -> Option<egui::Pos2> {
        match s {
            egui::Shape::Text(t) if t.galley.job.text == n => Some(t.pos + t.galley.size() / 2.),
            egui::Shape::Vec(v) => v.iter().find_map(|s| visit(s, n)),
            _ => None,
        }
    }
    shapes.iter().find_map(|s| visit(&s.shape, needle))
}
#[test]
fn timeline_pointer_steps_forward_and_back() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut selected = 0;
    let mut time = 0.;
    let mut frame = |selected: &mut usize, events| {
        time += 0.05;
        let mut o = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 300.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| scrub(ui, selected, 3));
            },
        );
        o.textures_delta.clear();
        o.shapes
    };
    for (label, expected) in [
        ("Next version", 1),
        ("Next version", 2),
        ("Previous version", 1),
    ] {
        let mut pos = None;
        for _ in 0..5 {
            pos = locate(&frame(&mut selected, vec![]), label);
            if pos.is_some() {
                break;
            }
        }
        let pos = pos.ok_or("scrub button painted")?;
        for pressed in [true, false] {
            frame(
                &mut selected,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(selected, expected);
    }
    Ok(())
}
