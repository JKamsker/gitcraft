//! Design tokens, modelled on PhotoCraft's "Pro" themes: flat grey panels, a darker tab strip,
//! a blue accent, Inter for the UI and JetBrains Mono for hashes and diffs.
//!
//! Widgets read [`Tokens::get`] instead of hard-coding colours, so every theme applies everywhere.
//! A theme is a [`Palette`]: the Pro themes and the well-known editor themes of [`catalog`],
//! your own theme files ([`files`]: gitcraft's, VS Code's, Base16), or VS Code's current
//! theme. [`Tweaks`] then change the accent, the corners and the graph's lanes of any of them.

pub mod catalog;
pub mod files;
pub mod palette;
#[cfg(test)]
pub(crate) mod test_paint;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Visuals,
};
use serde::{Deserialize, Serialize};

pub use palette::Palette;

/// Which theme: one of the catalogue by its id, `file:<name>` for a file in the themes folder,
/// or `vscode` for VS Code's current theme. Saved as the string, which is how the three themes
/// before were saved too.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ThemeId(pub String);

impl Default for ThemeId {
    fn default() -> Self {
        ThemeId("Dark".to_owned())
    }
}

impl<'de> Deserialize<'de> for ThemeId {
    /// A string, or the unit variant the themes were saved as when they were an enum (RON
    /// writes those bare: `theme: Light`). Untagged, RON hands over a bare name as a string.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Name(String),
        }
        let Saved::Name(name) = Saved::deserialize(d)?;
        Ok(ThemeId(name))
    }
}

impl ThemeId {
    pub const VSCODE: &'static str = "vscode";

    pub fn new(id: &str) -> Self {
        ThemeId(id.to_owned())
    }

    pub fn file(name: &str) -> Self {
        ThemeId(format!("file:{name}"))
    }

    /// The theme's name, or its id when there is no such theme (any more).
    pub fn label(&self) -> String {
        find(self).map_or_else(|| self.0.clone(), |t| t.name.clone())
    }
}

/// Where a theme comes from, which is how the theme lists group them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// gitcraft's own (the Pro themes, high contrast).
    Gitcraft,
    /// The palette of a well-known editor theme.
    Dark,
    Light,
    /// A file in the themes folder.
    Yours,
    /// VS Code's current theme.
    Editor,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Gitcraft,
        Group::Dark,
        Group::Light,
        Group::Yours,
        Group::Editor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::Gitcraft => "gitcraft",
            Group::Dark => crate::i18n::text("interaction-preferences-group-dark"),
            Group::Light => crate::i18n::text("interaction-preferences-group-light"),
            Group::Yours => crate::i18n::text("interaction-preferences-group-yours"),
            Group::Editor => crate::i18n::text("interaction-preferences-group-editor"),
        }
    }
}

/// A theme to choose.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeInfo {
    pub id: ThemeId,
    pub name: String,
    /// Who made it, where it is from.
    pub credit: String,
    pub group: Group,
    pub palette: Palette,
}

impl ThemeInfo {
    pub fn dark(&self) -> bool {
        self.palette.dark
    }

    pub fn tokens(&self, tweaks: &Tweaks) -> Tokens {
        let mut t = self.palette.tokens();
        tweaks.apply(&mut t);
        t
    }
}

mod registry;
pub use registry::*;

/// What can be changed about any theme.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tweaks {
    /// An accent of your own in place of the theme's.
    pub accent: Option<[u8; 3]>,
    /// How round corners are, 1.0 as the theme has them; 0 square.
    pub roundness: f32,
    /// The graph's lane colours.
    pub lanes: LaneColors,
}

impl Default for Tweaks {
    fn default() -> Self {
        Tweaks {
            accent: None,
            roundness: 1.0,
            lanes: LaneColors::Theme,
        }
    }
}

/// Where the graph's lanes take their colours from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneColors {
    /// The theme's own hues.
    #[default]
    Theme,
    /// gitcraft's bright lanes, the same in every theme.
    Classic,
    /// Okabe and Ito's colours, told apart with any colour blindness.
    ColorBlind,
}

impl LaneColors {
    pub const ALL: [LaneColors; 3] = [
        LaneColors::Theme,
        LaneColors::Classic,
        LaneColors::ColorBlind,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LaneColors::Theme => crate::i18n::text("interaction-presentation-lane-theme"),
            LaneColors::Classic => crate::i18n::text("interaction-presentation-lane-classic"),
            LaneColors::ColorBlind => crate::i18n::text("interaction-presentation-lane-colorblind"),
        }
    }
}

impl Tweaks {
    pub fn apply(&self, t: &mut Tokens) {
        if let Some([r, g, b]) = self.accent {
            let accent = Color32::from_rgb(r, g, b);
            t.accent = accent;
            t.accent_fg = accent;
            t.accent_text = palette::readable_on(accent);
            t.row_selected = t
                .dock
                .lerp_to_gamma(accent, if t.dark { 0.3 } else { 0.22 });
        }
        let r = self.roundness.clamp(0.0, 3.0);
        t.radius_sm = (3.0 * r).round();
        t.radius = (4.0 * r).round();
        t.radius_lg = (6.0 * r).round();
        match self.lanes {
            LaneColors::Theme => {}
            LaneColors::Classic => {
                t.lanes = if t.dark {
                    catalog::CLASSIC_DARK_LANES
                } else {
                    catalog::CLASSIC_LIGHT_LANES
                };
            }
            LaneColors::ColorBlind => {
                t.lanes = if t.dark {
                    catalog::COLOR_BLIND_DARK_LANES
                } else {
                    catalog::COLOR_BLIND_LIGHT_LANES
                };
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// Toolbars and the active tab.
    pub chrome: Color32,
    /// Behind panels (the graph area).
    pub dock: Color32,
    /// Side panels.
    pub card: Color32,
    /// Inputs and code blocks.
    pub field: Color32,
    pub field_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    /// Accent foreground for links and small labels, independent of button fills.
    pub accent_fg: Color32,
    pub separator: Color32,
    pub shadow: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub success: Color32,
    /// Window tab strip and panel headers.
    pub tab_strip: Color32,
    pub row_selected: Color32,
    pub diff_add_bg: Color32,
    pub diff_del_bg: Color32,
    /// The graph's lane colours, in order.
    pub lanes: [Color32; 10],
    /// The terminal's 16 colours.
    pub ansi: [Color32; 16],
    pub radius_sm: f32,
    pub radius: f32,
    pub radius_lg: f32,
}

impl Default for Tokens {
    /// gitcraft's dark theme.
    fn default() -> Self {
        catalog::BUILTIN[0].palette().tokens()
    }
}

impl Tokens {
    /// Intraline backgrounds follow the diff fill hue, independently from status text.
    pub(crate) fn diff_strong_bg(&self, added: bool) -> Color32 {
        let fill = if added {
            self.diff_add_bg
        } else {
            self.diff_del_bg
        };
        let [r, g, b, _] = fill.to_srgba_unmultiplied();
        let hue = Color32::from_rgb(r, g, b);
        let backgrounds: Vec<_> = [self.field, self.card]
            .into_iter()
            .flat_map(|surface| {
                let line = palette::over(fill, surface);
                [line, palette::over(self.accent.gamma_multiply(0.22), line)]
            })
            .collect();
        // A bright status foreground must never become a bright text background.
        // Reduce the independent tint when an opaque fill would obscure plain text.
        for alpha in (0..=89).rev() {
            let tint = hue.gamma_multiply(alpha as f32 / 255.0);
            if backgrounds
                .iter()
                .all(|bg| palette::contrast(self.text, palette::over(tint, *bg)) >= 4.5)
            {
                return tint;
            }
        }
        Color32::TRANSPARENT
    }

    /// Tint behind small status text, capped so the composited row still meets AA.
    pub(crate) fn text_tint(&self, color: Color32, opacity: f32, background: Color32) -> Color32 {
        palette::readable_tint(color, opacity, &[background], &[color])
    }

    /// Search-match shading shares a row with every kind of metadata and status label.
    pub(crate) fn match_tint(&self) -> Color32 {
        palette::readable_tint(
            self.warning,
            0.10,
            &[self.dock],
            &[
                self.text,
                self.text_dim,
                self.text_faint,
                self.accent_fg,
                self.danger,
                self.warning,
                self.success,
            ],
        )
    }

    pub fn dark(&self) -> bool {
        self.dark
    }

    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(tokens_id()))
            .unwrap_or_default()
    }

    /// Colour of a graph lane; lanes keep their colour for their whole life.
    pub fn lane(&self, lane: usize) -> Color32 {
        self.lanes[lane % self.lanes.len()]
    }
}

mod style;
pub use style::*;
