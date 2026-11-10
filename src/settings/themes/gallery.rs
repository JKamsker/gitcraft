//! Theme gallery filtering, selection and thumbnail layout.

use super::{ProfileRequest, SettingsView, theme_credit};
use crate::settings::{
    layout::{self, Filter},
    preview,
};
use crate::theme::{self, Group, Tokens};
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

/// Which themes the gallery shows.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Shown {
    #[default]
    All,
    Dark,
    Light,
}

const CARD: egui::Vec2 = egui::vec2(132.0, 82.0);
const NAME_H: f32 = 22.0;
const CARD_GAP: f32 = 12.0;

/// The gallery: a thumbnail of every theme, by group; clicking one uses it, pointing at one
/// shows it in the preview.
pub(super) fn gallery(ui: &mut Ui, f: &Filter, view: &mut SettingsView, tweaks: &theme::Tweaks) {
    let t = Tokens::get(ui.ctx());
    let shown_id = egui::Id::new("settings-theme-filter");
    let mut shown: Shown = ui.data(|d| d.get_temp(shown_id)).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        for (option, label) in [
            (Shown::All, crate::i18n::text("interaction-preferences-all")),
            (
                Shown::Dark,
                crate::i18n::text("interaction-preferences-dark"),
            ),
            (
                Shown::Light,
                crate::i18n::text("interaction-preferences-light"),
            ),
        ] {
            if ui.selectable_label(shown == option, label).clicked() {
                shown = option;
            }
        }
        layout::note(
            ui,
            crate::i18n::message(
                "interaction-preferences-count",
                &[("count", theme::themes().len().into())],
            ),
        );
    });
    ui.data_mut(|d| d.insert_temp(shown_id, shown));
    ui.add_space(6.0);
    let themes = theme::themes();
    // A search for a theme's name shows only the themes it finds.
    let named: Vec<bool> = themes.iter().map(|th| f.names(&th.name)).collect();
    let any_named = named.iter().any(|&n| n);
    let more_id = egui::Id::new("settings-themes-more");
    let active = theme::find(&view.theme);
    let external_active = active.as_ref().filter(|th| th.group != Group::Gitcraft);
    let external_count = themes
        .iter()
        .filter(|th| th.group != Group::Gitcraft)
        .count();
    // Search can reveal named external themes until the user explicitly collapses them.
    let search_external = any_named
        && themes
            .iter()
            .zip(&named)
            .any(|(th, named)| *named && th.group != Group::Gitcraft);
    // Reveal search matches automatically until the user chooses the disclosure state.
    // An explicit collapse wins over search, and the label always describes what is drawn.
    let mut more = ui
        .data(|d| d.get_temp::<bool>(more_id))
        .unwrap_or(search_external);
    for group in Group::ALL {
        if group == Group::Dark {
            ui.add_space(4.0);
            if ui
                .button(if more {
                    crate::i18n::text("dev-integration-ui-themes-less")
                } else {
                    crate::i18n::text("dev-integration-ui-themes-more")
                })
                .clicked()
            {
                more = !more;
                ui.data_mut(|d| d.insert_temp(more_id, more));
            }
            layout::note(
                ui,
                crate::i18n::message(
                    "dev-integration-ui-themes-external-count",
                    &[("count", external_count.into())],
                ),
            );
            if let Some(active) = external_active {
                layout::note(
                    ui,
                    crate::i18n::message(
                        "dev-integration-ui-theme-active",
                        &[("name", active.name.clone().into())],
                    ),
                );
            }
            if search_external && more {
                layout::note(ui, crate::i18n::text("dev-integration-ui-themes-search"));
            }
        }
        if group != Group::Gitcraft && !more {
            continue;
        }
        let members: Vec<_> = themes
            .iter()
            .zip(&named)
            .filter(|(th, named)| {
                th.group == group
                    && (!any_named || **named)
                    && match shown {
                        Shown::All => true,
                        Shown::Dark => th.dark(),
                        Shown::Light => !th.dark(),
                    }
            })
            .map(|(th, _)| th)
            .collect();
        if members.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(group.label().to_uppercase())
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.add_space(4.0);
        let per_row = ((ui.available_width() + CARD_GAP) / (CARD.x + CARD_GAP))
            .floor()
            .max(1.0) as usize;
        for chunk in members.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CARD_GAP;
                for info in chunk {
                    let card_w = CARD.x.min(ui.available_width().max(1.0));
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(card_w, CARD.y + NAME_H), Sense::click());
                    let thumb = Rect::from_min_size(rect.min, vec2(card_w, CARD.y));
                    let tokens = info.tokens(tweaks);
                    preview::thumbnail(ui.painter(), thumb, &tokens);
                    let chosen = info.id == view.theme;
                    crate::a11y::a11y_selected(&resp, &info.name, chosen);
                    if chosen {
                        ui.painter().rect_stroke(
                            thumb.expand(2.0),
                            8.0,
                            Stroke::new(2.0, t.accent),
                            egui::StrokeKind::Outside,
                        );
                    } else if resp.hovered() {
                        ui.painter().rect_stroke(
                            thumb.expand(2.0),
                            8.0,
                            Stroke::new(1.0, t.text_faint),
                            egui::StrokeKind::Outside,
                        );
                    }
                    let name_font = if chosen {
                        theme::medium(11.5)
                    } else {
                        FontId::proportional(11.5)
                    };
                    let name = fit(ui, &info.name, &name_font, card_w - 4.0);
                    ui.painter().text(
                        pos2(thumb.center().x, thumb.bottom() + NAME_H / 2.0 + 2.0),
                        Align2::CENTER_CENTER,
                        name,
                        name_font,
                        if chosen { t.text } else { t.text_dim },
                    );
                    if resp.hovered() {
                        preview::point_theme(ui.ctx(), &info.id);
                    }
                    let resp = resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("{}\n{}", info.name, theme_credit(info)));
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            chosen,
                            &info.name,
                        )
                    });
                    let keyboard = resp.has_focus()
                        && ui.input(|i| {
                            i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space)
                        });
                    if (resp.clicked() || keyboard) && !chosen {
                        view.request = Some(ProfileRequest::Theme(info.id.clone()));
                    }
                }
            });
            ui.add_space(CARD_GAP - 4.0);
        }
    }
}

fn fit(ui: &Ui, text: &str, font: &FontId, width: f32) -> String {
    let w = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    if w(text) <= width {
        return text.to_owned();
    }
    let mut s: String = text.to_owned();
    while !s.is_empty() && w(&format!("{s}…")) > width {
        s.pop();
    }
    format!("{}…", s.trim_end())
}
