//! Check the actual painter output, including text opacity and stacked translucent fills.

use egui::{Color32, Pos2, Rect, Shape, epaint::ClippedShape};

use super::palette::{contrast, over};

pub(crate) fn context() -> egui::Context {
    let ctx = egui::Context::default();
    super::install_fonts(&ctx);
    ctx.run_ui(Default::default(), |_| {})
        .drop_without_applying_deltas();
    ctx
}

pub(crate) fn assert_text_aa(shapes: &[ClippedShape], base: Color32, label: &str, context: &str) {
    fn visit(
        shape: &Shape,
        clip: Rect,
        backgrounds: &mut Vec<(Rect, Rect, Color32)>,
        base: Color32,
        label: &str,
        context: &str,
        found: &mut usize,
    ) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, clip, backgrounds, base, label, context, found);
                }
            }
            Shape::Rect(rect) => backgrounds.push((rect.rect, clip, rect.fill)),
            Shape::Text(text) if text.galley.job.text == label => {
                *found += 1;
                let center: Pos2 = text.visual_bounding_rect().center();
                let mut bg = base;
                for (rect, clip, color) in backgrounds.iter() {
                    if rect.contains(center) && clip.contains(center) {
                        bg = over(*color, bg);
                    }
                }
                for section in &text.galley.job.sections {
                    let color = text
                        .override_text_color
                        .unwrap_or_else(|| {
                            if section.format.color == Color32::PLACEHOLDER {
                                text.fallback_color
                            } else {
                                section.format.color
                            }
                        })
                        .gamma_multiply(text.opacity_factor);
                    let fg = over(color, bg);
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{context}: {label:?} painted {fg:?} on {bg:?}: {ratio:.3}:1"
                    );
                }
            }
            _ => {}
        }
    }
    let mut backgrounds = Vec::new();
    let mut found = 0;
    for clipped in shapes {
        visit(
            &clipped.shape,
            clipped.clip_rect,
            &mut backgrounds,
            base,
            label,
            context,
            &mut found,
        );
    }
    assert!(found > 0, "{context}: no painted {label:?} was checked");
}

#[test]
fn enabled_text_field_hints_use_the_actual_aa_weak_colour() {
    let ctx = context();
    for builtin in super::catalog::BUILTIN {
        let t = builtin.palette().tokens();
        super::set_tokens(&ctx, t);
        let mut value = String::new();
        let mut output = ctx.run_ui(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut value).hint_text("Summary hint"));
                ui.add(egui::TextEdit::multiline(&mut value).hint_text("Description hint"));
            });
        });
        output.textures_delta.clear();
        for label in ["Summary hint", "Description hint"] {
            assert_text_aa(&output.shapes, t.card, label, builtin.name);
        }
    }
}
