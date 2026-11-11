//! Working-file list headings and empty states.

use super::*;

/// A list title with a count and an optional text button on the right. Returns the click.
pub(super) fn list_header(ui: &mut Ui, title: &str, count: usize, button: Option<&str>) -> bool {
    list_header_with(ui, title, count, button, |_| {})
}

pub(super) fn list_header_with(
    ui: &mut Ui,
    title: &str,
    count: usize,
    button: Option<&str>,
    controls: impl FnOnce(&mut Ui),
) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut clicked = false;
    egui::Frame::new()
        .fill(t.tab_strip)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    controls(ui);
                    if let Some(label) = button {
                        clicked = ui
                            .small_button(label)
                            .on_hover_text(format!("{label} · {title}"))
                            .clicked();
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{title}  {count}"))
                                    .font(theme::semibold(11.0))
                                    .color(t.text_dim),
                            )
                            .truncate(),
                        );
                    });
                });
            });
        });
    clicked
}

pub(super) fn empty_hint(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(11.5),
        t.text_faint,
    );
}
