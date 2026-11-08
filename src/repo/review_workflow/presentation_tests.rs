//! Inert UI-only helpers: no application, repository, readers, discovery or workers.
use egui::{Context, Event, FullOutput, Pos2, RawInput, Rect, Vec2, epaint::Shape};

pub(super) fn frame(
    ctx: &Context,
    events: Vec<Event>,
    draw: &mut impl FnMut(&mut egui::Ui),
) -> FullOutput {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1800., 4000.))),
            events,
            ..Default::default()
        },
        draw,
    );
    // No renderer is created: acknowledge the atlas update in this inert fixture.
    output.textures_delta.clear();
    output
}
pub(super) fn text(output: &FullOutput) -> String {
    fn collect(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(t) => out.push(t.galley.text().to_owned()),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut strings = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut strings);
    }
    strings.join("\n")
}
pub(super) fn paint(ctx: &Context, mut draw: impl FnMut(&mut egui::Ui)) -> String {
    let _ = frame(ctx, vec![], &mut draw);
    text(&frame(ctx, vec![], &mut draw))
}
pub(super) fn click(
    ctx: &Context,
    label: &str,
    mut draw: impl FnMut(&mut egui::Ui),
) -> Result<(), String> {
    let _ = frame(ctx, vec![], &mut draw);
    let output = frame(ctx, vec![], &mut draw);
    let position = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let Shape::Text(t) = &shape.shape
                && t.galley.text() == label
            {
                Some(t.pos + t.galley.size() / 2.)
            } else {
                None
            }
        })
        .ok_or_else(|| format!("Missing button text: {label}\n{}", text(&output)))?;
    for pressed in [true, false] {
        let _ = frame(
            ctx,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut draw,
        );
    }
    Ok(())
}
