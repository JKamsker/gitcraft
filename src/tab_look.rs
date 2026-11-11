//! A repository tab's own icon and highlight colour, picked from the tab's right-click menu.

use egui::{Color32, Rect, RichText, Sense, Stroke, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::icons;
use crate::theme::{self, Tokens};

/// The icons a tab can wear: (asset key, description message ID). The first is the default.
pub const ICONS: &[(&str, &str)] = &[
    ("folder-git-2", "interaction-tab-look-icon-0"),
    ("code", "interaction-tab-look-icon-1"),
    ("globe", "interaction-tab-look-icon-2"),
    ("smartphone", "interaction-tab-look-icon-3"),
    ("server", "interaction-tab-look-icon-4"),
    ("database", "interaction-tab-look-icon-5"),
    ("package", "interaction-tab-look-icon-6"),
    ("square-terminal", "interaction-tab-look-icon-7"),
    ("rocket", "interaction-tab-look-icon-8"),
    ("gamepad-2", "interaction-tab-look-icon-9"),
    ("flask-conical", "interaction-tab-look-icon-10"),
    ("book-open", "interaction-tab-look-icon-11"),
    ("palette", "interaction-tab-look-icon-12"),
    ("wrench", "interaction-tab-look-icon-13"),
    ("star", "interaction-tab-look-icon-14"),
    ("heart", "interaction-tab-look-icon-15"),
];

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabColor {
    #[default]
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Purple,
    Gray,
}

impl TabColor {
    pub const ALL: [TabColor; 9] = [
        TabColor::Blue,
        TabColor::Teal,
        TabColor::Green,
        TabColor::Yellow,
        TabColor::Orange,
        TabColor::Red,
        TabColor::Pink,
        TabColor::Purple,
        TabColor::Gray,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TabColor::Blue => crate::i18n::text("interaction-tab-look-blue"),
            TabColor::Teal => crate::i18n::text("interaction-tab-look-teal"),
            TabColor::Green => crate::i18n::text("interaction-tab-look-green"),
            TabColor::Yellow => crate::i18n::text("interaction-tab-look-yellow"),
            TabColor::Orange => crate::i18n::text("interaction-tab-look-orange"),
            TabColor::Red => crate::i18n::text("interaction-tab-look-red"),
            TabColor::Pink => crate::i18n::text("interaction-tab-look-pink"),
            TabColor::Purple => crate::i18n::text("interaction-tab-look-purple"),
            TabColor::Gray => crate::i18n::text("interaction-tab-look-gray"),
        }
    }

    /// Blue is the theme's accent; the others are the same in every theme.
    pub fn color(self, t: &Tokens) -> Color32 {
        match self {
            TabColor::Blue => t.accent,
            TabColor::Teal => Color32::from_rgb(20, 178, 160),
            TabColor::Green => Color32::from_rgb(64, 180, 96),
            TabColor::Yellow => Color32::from_rgb(230, 178, 20),
            TabColor::Orange => Color32::from_rgb(240, 128, 40),
            TabColor::Red => Color32::from_rgb(230, 72, 72),
            TabColor::Pink => Color32::from_rgb(232, 80, 156),
            TabColor::Purple => Color32::from_rgb(150, 104, 236),
            TabColor::Gray => Color32::from_rgb(140, 142, 152),
        }
    }
}

/// How a repository's tab looks.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabLook {
    /// One of [`ICONS`]; `None` for the default.
    pub icon: Option<String>,
    pub color: TabColor,
}

impl TabLook {
    pub fn icon(&self) -> &str {
        self.icon.as_deref().unwrap_or(ICONS[0].0)
    }
}

/// The menu's icon grid and colour swatches; returns the look picked, if any.
pub fn menu(ui: &mut Ui, look: &TabLook) -> Option<TabLook> {
    let t = Tokens::get(ui.ctx());
    let accent = look.color.color(&t);
    let mut picked = None;
    let heading = |ui: &mut Ui, text: &str| {
        ui.label(
            RichText::new(text)
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
    };

    // Menus have no side margin of their own (their buttons pad themselves); the grids get one.
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            heading(ui, crate::i18n::text("interaction-tab-look-icons"));
            for row in ICONS.chunks(8) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for &(name, what) in row {
                        let what = crate::i18n::text(what);
                        let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                        let current = look.icon() == name;
                        crate::repo::widgets::a11y_selected(
                            &resp,
                            || {
                                crate::i18n::message(
                                    "interaction-tab-look-icon-name",
                                    &[("description", what.into())],
                                )
                            },
                            current,
                        );
                        if current {
                            ui.painter().rect_filled(
                                rect,
                                t.radius_sm,
                                accent.gamma_multiply(0.22),
                            );
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, t.radius_sm, t.hover);
                        }
                        let tint = if current { accent } else { t.text_dim };
                        icons::paint(
                            ui,
                            Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
                            name,
                            15.0,
                            tint,
                        );
                        if resp.on_hover_text(what).clicked() {
                            picked = Some(TabLook {
                                icon: (name != ICONS[0].0).then(|| name.to_owned()),
                                ..look.clone()
                            });
                        }
                    }
                });
            }

            ui.add_space(6.0);
            heading(ui, crate::i18n::text("interaction-tab-look-highlight"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for color in TabColor::ALL {
                    let (rect, resp) = ui.allocate_exact_size(vec2(25.0, 25.0), Sense::click());
                    crate::repo::widgets::a11y_selected(
                        &resp,
                        || {
                            crate::i18n::message(
                                "interaction-tab-look-highlight-color",
                                &[("color", color.label().into())],
                            )
                        },
                        color == look.color,
                    );
                    let c = rect.center();
                    if color == look.color {
                        ui.painter()
                            .circle_stroke(c, 10.0, Stroke::new(1.5, t.text));
                    } else if resp.hovered() {
                        ui.painter()
                            .circle_stroke(c, 10.0, Stroke::new(1.5, t.text_faint));
                    }
                    ui.painter().circle_filled(c, 7.0, color.color(&t));
                    if resp.on_hover_text(color.label()).clicked() {
                        picked = Some(TabLook {
                            color,
                            ..look.clone()
                        });
                    }
                }
            });
        });

    if *look != TabLook::default() {
        ui.separator();
        if crate::menu_item::MenuItem::new(crate::i18n::text("interaction-tab-look-reset"))
            .icon("undo-2")
            .show(ui)
            .clicked()
        {
            picked = Some(TabLook::default());
        }
    }
    picked
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_icon_and_highlight_menu_preserves_persisted_icon_and_color_values()
    -> Result<(), String> {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, look: &mut TabLook| {
                if !fonts.replace(true) {
                    theme::install_fonts(ui.ctx());
                    return;
                }
                if let Some(picked) = menu(ui, look) {
                    *look = picked;
                }
            },
            TabLook::default(),
        );
        h.run();
        h.get_by_label("Symbol Bibliothek").click();
        h.run();
        assert_eq!(h.state().icon.as_deref(), Some("package"));
        h.get_by_label("Hervorhebung Rot").click();
        h.run();
        assert!(h.state().color == TabColor::Red);
        let saved = serde_json::to_string(h.state()).map_err(|error| format!("{error:?}"))?;
        assert_eq!(saved, r#"{"icon":"package","color":"Red"}"#);
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("Icon Library");
        h.get_by_label("Highlight Red");
        assert_eq!(
            serde_json::to_string(h.state()).map_err(|error| format!("{error:?}"))?,
            saved
        );

        Ok(())
    }
}
