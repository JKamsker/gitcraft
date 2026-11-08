//! Choosing a theme: the gallery on the Appearance page, the theme menu of the title bar, and
//! what can be changed about any theme (accent, corners) and done with theme files.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::layout::{self, Card, Filter};
use super::preview::{self, Preview};
use super::{ProfileRequest, Settings, SettingsView};
use crate::theme::{self, Group, ThemeId, Tokens};

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
fn gallery(ui: &mut Ui, f: &Filter, view: &mut SettingsView, tweaks: &theme::Tweaks) {
    let t = Tokens::get(ui.ctx());
    let shown_id = egui::Id::new("settings-theme-filter");
    let mut shown: Shown = ui.data(|d| d.get_temp(shown_id)).unwrap_or_default();
    ui.horizontal(|ui| {
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
    for group in Group::ALL {
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
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(CARD.x, CARD.y + NAME_H), Sense::click());
                    let thumb = Rect::from_min_size(rect.min, CARD);
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
                    let name = fit(ui, &info.name, &name_font, CARD.x - 4.0);
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
                        .on_hover_text(format!("{}\n{}", info.name, info.credit));
                    if resp.clicked() && !chosen {
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

/// Accents to pick from besides the theme's own.
const ACCENTS: [[u8; 3]; 9] = [
    [55, 142, 240],
    [124, 92, 246],
    [217, 70, 168],
    [229, 72, 77],
    [240, 128, 48],
    [222, 172, 30],
    [46, 170, 92],
    [20, 160, 150],
    [30, 170, 210],
];

impl Settings {
    /// The Theme section of the Appearance page.
    pub(super) fn themes_ui(&mut self, card: &mut Card, f: &Filter, view: &mut SettingsView) {
        let names: String = theme::themes()
            .iter()
            .map(|th| th.name.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        card.preview(Preview::Theme);
        let tweaks = self.theme_tweaks;
        card.full(
            &format!("theme themes gallery colours colors dark light {names}"),
            |ui| {
                gallery(ui, f, view, &tweaks);
            },
        );
        card.preview(Preview::Accent);
        card.row(
            "Accent colour",
            "accent highlight colour color buttons selection",
            |ui| {
                let ctx = ui.ctx().clone();
                let t = Tokens::get(&ctx);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let own = theme::resolve(&view.theme).palette.accent;
                    if swatch(ui, own, self.theme_tweaks.accent.is_none(), &t)
                        .on_hover_text(crate::i18n::text("interaction-preferences-own"))
                        .clicked()
                    {
                        self.theme_tweaks.accent = None;
                    }
                    ui.add_space(6.0);
                    for rgb in ACCENTS {
                        let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        if swatch(ui, c, self.theme_tweaks.accent == Some(rgb), &t).clicked() {
                            self.theme_tweaks.accent = Some(rgb);
                        }
                    }
                    let mut rgb = self
                        .theme_tweaks
                        .accent
                        .unwrap_or([own.r(), own.g(), own.b()]);
                    if egui::color_picker::color_edit_button_srgb(ui, &mut rgb)
                        .on_hover_text(crate::i18n::text("interaction-preferences-any"))
                        .changed()
                    {
                        self.theme_tweaks.accent = Some(rgb);
                    }
                });
                layout::note(
                    ui,
                    if self.theme_tweaks.accent.is_none() {
                        crate::i18n::text("interaction-preferences-own")
                    } else {
                        crate::i18n::text("interaction-preferences-your-accent")
                    },
                );
            },
        );
        card.preview(Preview::Corners);
        card.row("Corners", "roundness radius rounded square corner", |ui| {
            ui.spacing_mut().slider_width = layout::fit(ui, 240.0 + 70.0) - 70.0;
            ui.add(layout::slider(
                egui::Slider::new(&mut self.theme_tweaks.roundness, 0.0..=2.5)
                    .step_by(0.25)
                    .custom_formatter(|v, _| match v {
                        v if v <= 0.0 => {
                            crate::i18n::text("interaction-preferences-square").to_owned()
                        }
                        v if (v - 1.0).abs() < 0.01 => {
                            crate::i18n::text("interaction-preferences-as-theme").to_owned()
                        }
                        v => format!("{:.0} %", v * 100.0),
                    }),
            ));
        });
        card.preview(Preview::Theme);
        card.row_hint(
            "Your themes",
            crate::i18n::text("interaction-preferences-your-themes"),
            "import file folder vscode base16 base24 json yaml custom edit export copy",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(crate::i18n::text("interaction-preferences-import"))
                        .on_hover_text(crate::i18n::text("interaction-preferences-import-help"))
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title(crate::i18n::text("interaction-preferences-import-title"))
                            .add_filter(
                                crate::i18n::text("interaction-preferences-theme-files"),
                                &["json", "jsonc", "yaml", "yml"],
                            )
                            .pick_file()
                    {
                        match theme::files::import(&path) {
                            Ok(file) => {
                                theme::refresh();
                                view.request = Some(ProfileRequest::Theme(ThemeId::file(&file)));
                                set_message(
                                    ui.ctx(),
                                    Ok(crate::i18n::message(
                                        "interaction-preferences-imported",
                                        &[("file", file.into())],
                                    )),
                                );
                            }
                            Err(e) => set_message(
                                ui.ctx(),
                                Err(crate::i18n::message(
                                    "interaction-preferences-not-imported",
                                    &[("error", e.into())],
                                )),
                            ),
                        }
                    }
                    if ui
                        .button(crate::i18n::text("interaction-preferences-copy"))
                        .on_hover_text(crate::i18n::text("interaction-preferences-copy-help"))
                        .clicked()
                    {
                        let info = theme::resolve(&view.theme);
                        let mut palette = info.palette;
                        if let Some([r, g, b]) = self.theme_tweaks.accent {
                            palette.accent = Color32::from_rgb(r, g, b);
                            palette.accent_text = None;
                            palette.row_selected = None;
                        }
                        let extends = theme::catalog::BUILTIN
                            .iter()
                            .find(|b| b.id == info.id.0)
                            .map(|b| b.id);
                        match theme::files::save_copy(
                            &format!("{} (mine)", info.name),
                            &palette,
                            extends,
                        ) {
                            Ok(file) => {
                                theme::refresh();
                                view.request = Some(ProfileRequest::Theme(ThemeId::file(&file)));
                                set_message(
                                    ui.ctx(),
                                    Ok(crate::i18n::message(
                                        "interaction-preferences-saved",
                                        &[("file", file.into())],
                                    )),
                                );
                            }
                            Err(e) => set_message(
                                ui.ctx(),
                                Err(crate::i18n::message(
                                    "interaction-preferences-not-saved",
                                    &[("error", e.into())],
                                )),
                            ),
                        }
                    }
                    if ui
                        .button(crate::i18n::text("interaction-preferences-open-folder"))
                        .clicked()
                        && let Some(dir) = theme::files::dir()
                    {
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = open::that_detached(dir);
                    }
                });
                let t = Tokens::get(ui.ctx());
                if let Some(message) = message(ui.ctx()) {
                    match message {
                        Ok(m) => ui.label(RichText::new(m).color(t.success).small()),
                        Err(m) => ui.label(RichText::new(m).color(t.danger).small()),
                    };
                }
                for (file, why) in theme::theme_errors() {
                    ui.label(
                        RichText::new(format!("{file}: {why}"))
                            .color(t.warning)
                            .small(),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    layout::note(ui, crate::i18n::text("interaction-preferences-more"));
                    ui.hyperlink_to(
                        RichText::new(crate::i18n::text("interaction-preferences-vscode")).small(),
                        "https://vscodethemes.com",
                    );
                    layout::note(ui, "·");
                    ui.hyperlink_to(
                        RichText::new(crate::i18n::text("interaction-preferences-base16")).small(),
                        "https://github.com/tinted-theming/schemes",
                    );
                });
            },
        );
    }
}

/// A round swatch of `c`, ringed when `chosen`.
fn swatch(ui: &mut Ui, c: Color32, chosen: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    crate::a11y::a11y_selected(
        &resp,
        || {
            crate::i18n::message(
                "interaction-preferences-colour",
                &[(
                    "hex",
                    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()).into(),
                )],
            )
        },
        chosen,
    );
    let p = ui.painter();
    if chosen {
        p.circle_stroke(rect.center(), 10.5, Stroke::new(2.0, t.text));
    } else if resp.hovered() {
        p.circle_stroke(rect.center(), 10.5, Stroke::new(1.0, t.text_faint));
    }
    p.circle_filled(rect.center(), 8.0, c);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn message_id() -> egui::Id {
    egui::Id::new("settings-theme-message")
}

/// What the last import or copy did, shown under its buttons for a while.
fn set_message(ctx: &egui::Context, m: Result<String, String>) {
    let at = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(message_id(), (at, m)));
}

fn message(ctx: &egui::Context) -> Option<Result<String, String>> {
    let now = ctx.input(|i| i.time);
    ctx.data(|d| d.get_temp::<(f64, Result<String, String>)>(message_id()))
        .filter(|(at, _)| now - at < 8.0)
        .map(|(_, m)| m)
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_colour_swatch_translates_name_and_retains_rgb_and_selection() {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let colour = Color32::from_rgb(0x23, 0x68, 0xA2);
        let mut h = Harness::new_ui_state(
            move |ui, chosen: &mut bool| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                if swatch(ui, colour, *chosen, &t).clicked() {
                    *chosen = !*chosen;
                }
            },
            false,
        );
        h.run();
        h.get_by_label("Farbe #2368A2").click();
        h.run();
        assert!(*h.state());
        assert_eq!(colour.to_array(), [0x23, 0x68, 0xA2, 0xFF]);
        assert_eq!(Group::Dark.label(), "Dunkel");
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Colour #2368A2");
        assert!(*h.state());
        assert_eq!(Group::Dark.label(), "Dark");
    }
}
