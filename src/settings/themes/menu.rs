//! Compact theme picker in the title bar.

use crate::theme::{self, Group, ThemeId, Tokens};
use egui::{Align2, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

/// What the title bar's theme menu picked.
#[derive(Clone, Debug)]
pub enum ThemePick {
    Theme(ThemeId),
    /// The settings' themes, to see them all and more.
    Browse,
}

/// The theme menu: every theme by group, with its colours. Returns what was picked.
pub fn theme_menu(ui: &mut Ui, current: &ThemeId) -> Option<ThemePick> {
    let t = Tokens::get(ui.ctx());
    let mut pick = None;
    let themes = theme::themes();
    let max_h = (ui.ctx().content_rect().height() - 140.0).clamp(200.0, 520.0);
    ui.set_width(270.0);
    egui::ScrollArea::vertical()
        .max_height(max_h)
        .show(ui, |ui| {
            for group in Group::ALL {
                let mut members = themes.iter().filter(|th| th.group == group).peekable();
                if members.peek().is_none() {
                    continue;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(group.label().to_uppercase())
                            .font(theme::semibold(10.0))
                            .color(t.text_faint),
                    );
                });
                ui.add_space(2.0);
                for info in members {
                    let chosen = info.id == *current;
                    let tokens = info.palette.tokens();
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                    crate::a11y::a11y_selected(&resp, &info.name, chosen);
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, 0.0, t.hover);
                    }
                    let p = ui.painter();
                    // Its background, accent and three lanes.
                    let sw = Rect::from_min_size(
                        pos2(rect.left() + 16.0, rect.center().y - 7.0),
                        vec2(30.0, 14.0),
                    );
                    p.rect_filled(sw, 3.0, tokens.dock);
                    p.rect_stroke(
                        sw,
                        3.0,
                        Stroke::new(1.0, t.field_border),
                        egui::StrokeKind::Inside,
                    );
                    for (k, c) in [tokens.accent, tokens.lane(1), tokens.lane(2)]
                        .iter()
                        .enumerate()
                    {
                        p.circle_filled(
                            pos2(sw.left() + 7.0 + k as f32 * 8.0, sw.center().y),
                            3.0,
                            *c,
                        );
                    }
                    p.text(
                        pos2(sw.right() + 10.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        &info.name,
                        if chosen {
                            theme::medium(12.5)
                        } else {
                            FontId::proportional(12.5)
                        },
                        t.text,
                    );
                    if chosen {
                        let c = pos2(rect.right() - 18.0, rect.center().y);
                        p.add(egui::Shape::line(
                            vec![
                                c + vec2(-4.0, 0.0),
                                c + vec2(-1.0, 3.0),
                                c + vec2(4.5, -3.5),
                            ],
                            Stroke::new(1.8, t.accent),
                        ));
                    }
                    if resp.clicked() {
                        pick = Some(ThemePick::Theme(info.id.clone()));
                    }
                }
            }
        });
    ui.separator();
    if ui
        .button(crate::i18n::text("interaction-preferences-browse"))
        .clicked()
    {
        pick = Some(ThemePick::Browse);
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}
