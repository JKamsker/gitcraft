use super::*;
#[test]
fn pure_bakeoff_unavailable_and_running_cells_offer_explicit_fresh_workspace_retry()
-> Result<(), String> {
    for built in [Built::Unavailable, Built::Running] {
        let ctx = egui::Context::default();
        let mut act = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                let tokens = Tokens::get(ui.ctx());
                build_cell(ui, Some(built), false, 0, &tokens, &mut act);
            },
        );
        let position = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Retry in fresh workspace" => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        });
        output.textures_delta.clear();
        let position = position.ok_or("retry remains reachable without a completion receipt")?;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                let tokens = Tokens::get(ui.ctx());
                build_cell(ui, Some(built), false, 0, &tokens, &mut act);
            },
        );
        output.textures_delta.clear();
        assert!(matches!(act, Some(BakeAct::Build(0))));
    }
    Ok(())
}
