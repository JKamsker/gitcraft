//! Choosing a theme: the gallery on the Appearance page, the theme menu of the title bar, and
//! what can be changed about any theme (accent, corners) and done with theme files.

use egui::{Color32, RichText, Sense, Stroke, Ui, vec2};

use super::layout::{self, Card, Filter};
use super::preview::Preview;
use super::{ProfileRequest, Settings, SettingsView};
use crate::theme::{self, Group, ThemeId, Tokens};

mod menu;
pub use menu::{ThemePick, theme_menu};
mod gallery;
use gallery::gallery;
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
                                    &[("error", theme::files::display_diagnostic(&e).into())],
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
                                    &[("error", theme::files::display_diagnostic(&e).into())],
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
                        RichText::new(theme_file_error(&file, &why))
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

/// Only file-generated credit kinds are presentation; built-in attribution is opaque.
fn theme_credit(info: &theme::ThemeInfo) -> String {
    if info.group == Group::Yours {
        theme::files::display_credit(&info.credit)
    } else {
        info.credit.clone()
    }
}

fn theme_file_error(file: &str, raw: &str) -> String {
    crate::i18n::message(
        "interaction-theme-file-file-error",
        &[
            ("file", file.into()),
            ("error", theme::files::display_diagnostic(raw).into()),
        ],
    )
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
#[path = "themes/localization_tests.rs"]
mod localization_tests;

#[cfg(test)]
#[path = "themes/tests.rs"]
mod tests;
