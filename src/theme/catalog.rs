//! The themes that come with gitcraft: PhotoCraft's three Pro themes, and the palettes of
//! well-known editor themes, taken from where each is published (see ATTRIBUTION.md).

use egui::Color32;

use super::palette::{Palette, aa_foreground, hex, over};

/// A theme of the catalogue.
pub struct Builtin {
    /// What it is saved as. The Pro themes keep the names they were saved by before there were
    /// more themes.
    pub id: &'static str,
    pub name: &'static str,
    /// Who made the palette, and where it is published.
    pub credit: &'static str,
    // Source palette for imports and inheritance, which keep the author's colours.
    pub(super) palette: Palette,
}

impl Builtin {
    /// Built-in UI text meets WCAG AA even at the small sizes of graph metadata and hints.
    /// Keep the published surfaces, terminal and lane colours; user themes are never adjusted.
    pub fn palette(&self) -> Palette {
        let mut p = self.palette;
        let t = p.tokens();
        // Freeze derived backgrounds before raising text or changing the button fill.
        p.hover = Some(t.hover);
        p.pressed = Some(t.pressed);
        p.row_selected = Some(t.row_selected);
        p.separator = Some(t.separator);
        p.icon = Some(t.icon);
        p.lanes = Some(t.lanes);
        p.ansi = Some(t.ansi);
        let surfaces = [
            t.tab_strip,
            t.dock,
            t.card,
            t.field,
            t.hover,
            t.pressed,
            t.row_selected,
        ];
        // Preserve primary-button label contrast before deriving picked diff-line tints.
        p.accent = aa_foreground(p.accent, &[t.accent_text]);
        let mut text_surfaces = surfaces.to_vec();
        for bg in [
            t.field,
            t.card,
            over(t.diff_add_bg, t.field),
            over(t.diff_del_bg, t.field),
            over(t.diff_add_bg, t.card),
            over(t.diff_del_bg, t.card),
        ] {
            text_surfaces.push(bg);
            text_surfaces.push(over(p.accent.gamma_multiply(0.22), bg));
        }
        p.text = aa_foreground(p.text, &text_surfaces);
        // Pressed controls use normal or dim labels. Faint metadata belongs to rows/panels;
        // keep its hierarchy without adjusting it for icon-only pressed backgrounds.
        let quiet_surfaces = [
            t.tab_strip,
            t.dock,
            t.card,
            t.field,
            t.hover,
            t.row_selected,
        ];
        p.text_dim = aa_foreground(p.text_dim, &surfaces);
        p.text_faint = aa_foreground(p.text_faint, &quiet_surfaces);
        p.accent_text = Some(t.accent_text);
        let tinted = |mut color: Color32, fill: Color32| {
            // Chips sit on panels; diff hunk labels sit on a tinted field.
            loop {
                let mut backgrounds = quiet_surfaces.to_vec();
                for bg in [t.tab_strip, t.dock, t.card, t.field] {
                    backgrounds.push(over(color.gamma_multiply(0.18), bg));
                }
                backgrounds.push(over(fill.gamma_multiply(0.22), t.field));
                let raised = aa_foreground(color, &backgrounds);
                if raised == color {
                    return color;
                }
                color = raised;
            }
        };
        p.accent_fg = Some(tinted(t.accent_fg, p.accent));
        p.danger = Some(tinted(t.danger, t.danger));
        p.warning = Some(tinted(t.warning, t.warning));
        p.success = Some(tinted(t.success, t.success));
        p
    }
}

const fn pal(
    dark: bool,
    surfaces: [u32; 5],
    text: [u32; 3],
    accent: u32,
    hues: [u32; 7],
) -> Palette {
    Palette::new(
        dark,
        [
            hex(surfaces[0]),
            hex(surfaces[1]),
            hex(surfaces[2]),
            hex(surfaces[3]),
            hex(surfaces[4]),
        ],
        [hex(text[0]), hex(text[1]), hex(text[2])],
        hex(accent),
        [
            hex(hues[0]),
            hex(hues[1]),
            hex(hues[2]),
            hex(hues[3]),
            hex(hues[4]),
            hex(hues[5]),
            hex(hues[6]),
        ],
    )
}

const fn ansi(c: [u32; 16]) -> [Color32; 16] {
    let mut out = [Color32::BLACK; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = hex(c[i]);
        i += 1;
    }
    out
}

/// The graph's lanes before there were more themes, for the Pro themes and the "Classic" lane
/// colours.
pub const CLASSIC_DARK_LANES: [Color32; 10] = [
    Color32::from_rgb(64, 156, 255),
    Color32::from_rgb(255, 120, 80),
    Color32::from_rgb(90, 200, 120),
    Color32::from_rgb(200, 110, 230),
    Color32::from_rgb(250, 200, 60),
    Color32::from_rgb(60, 205, 210),
    Color32::from_rgb(240, 90, 150),
    Color32::from_rgb(160, 200, 70),
    Color32::from_rgb(150, 140, 255),
    Color32::from_rgb(230, 150, 70),
];
pub const CLASSIC_LIGHT_LANES: [Color32; 10] = [
    Color32::from_rgb(20, 110, 220),
    Color32::from_rgb(220, 80, 40),
    Color32::from_rgb(30, 150, 70),
    Color32::from_rgb(150, 60, 190),
    Color32::from_rgb(200, 140, 0),
    Color32::from_rgb(0, 150, 160),
    Color32::from_rgb(200, 40, 110),
    Color32::from_rgb(110, 150, 20),
    Color32::from_rgb(90, 80, 220),
    Color32::from_rgb(190, 100, 20),
];

/// Okabe and Ito's colours, told apart with every kind of colour blindness, and two greys.
pub const COLOR_BLIND_DARK_LANES: [Color32; 10] = [
    hex(0x56b4e9),
    hex(0xe69f00),
    hex(0x009e73),
    hex(0xcc79a7),
    hex(0xf0e442),
    hex(0x0072b2),
    hex(0xd55e00),
    hex(0xbbbbbb),
    hex(0x7fc9f0),
    hex(0xf2c46d),
];
pub const COLOR_BLIND_LIGHT_LANES: [Color32; 10] = [
    hex(0x0072b2),
    hex(0xd55e00),
    hex(0x009e73),
    hex(0xcc79a7),
    hex(0xb89a00),
    hex(0x56b4e9),
    hex(0xe69f00),
    hex(0x666666),
    hex(0x004c78),
    hex(0x8f3e00),
];

/// The terminal's colours of the Pro themes.
const PRO_DARK_ANSI: [Color32; 16] = ansi([
    0x282828, 0xec5b62, 0x5cc47a, 0xe8b046, 0x409cff, 0xc86ee6, 0x3ccdd2, 0xc8c8c8, 0x6e6e6e,
    0xff787d, 0x82e196, 0xfacd64, 0x6eb4ff, 0xdc96f5, 0x6ee1e6, 0xf5f5f5,
]);
const PRO_LIGHT_ANSI: [Color32; 16] = ansi([
    0x141414, 0xc82832, 0x1e8c46, 0xaa6e00, 0x1464d2, 0x963cbe, 0x008c96, 0x6e6e6e, 0x5a5a5a,
    0xe63c46, 0x28aa5a, 0xc88c00, 0x2878f0, 0xb450dc, 0x00aab4, 0x282828,
]);

/// PhotoCraft's Pro (Dark), before the small-text contrast adjustment.
const fn pro_dark() -> Palette {
    let mut p = Palette::new(
        true,
        [
            Color32::from_rgb(38, 38, 38),
            Color32::from_rgb(40, 40, 40),
            Color32::from_rgb(50, 50, 50),
            Color32::from_rgb(36, 36, 36),
            Color32::from_rgb(74, 74, 74),
        ],
        [
            Color32::from_rgb(222, 222, 222),
            Color32::from_rgb(178, 178, 178),
            Color32::from_rgb(128, 128, 128),
        ],
        Color32::from_rgb(55, 142, 240),
        [
            Color32::from_rgb(236, 91, 98),
            Color32::from_rgb(92, 196, 122),
            Color32::from_rgb(232, 176, 70),
            Color32::from_rgb(64, 156, 255),
            Color32::from_rgb(200, 110, 230),
            Color32::from_rgb(60, 205, 210),
            Color32::from_rgb(255, 120, 80),
        ],
    );
    p.hover = Some(Color32::from_rgb(66, 66, 66));
    p.pressed = Some(Color32::from_rgb(78, 78, 78));
    p.separator = Some(Color32::from_rgb(30, 30, 30));
    p.icon = Some(Color32::from_rgb(200, 200, 200));
    p.accent_text = Some(Color32::WHITE);
    p.row_selected = Some(Color32::from_rgb(38, 76, 122));
    p.diff_add_bg = Some(Color32::from_rgba_unmultiplied_const(92, 196, 122, 34));
    p.diff_del_bg = Some(Color32::from_rgba_unmultiplied_const(236, 91, 98, 34));
    p.lanes = Some(CLASSIC_DARK_LANES);
    p.ansi = Some(PRO_DARK_ANSI);
    p
}

const fn pro_medium_gray() -> Palette {
    let mut p = pro_dark();
    p.tab_strip = Color32::from_rgb(74, 74, 74);
    p.dock = Color32::from_rgb(66, 66, 66);
    p.chrome = Color32::from_rgb(83, 83, 83);
    p.field = Color32::from_rgb(69, 69, 69);
    p.border = Color32::from_rgb(104, 104, 104);
    p.text = Color32::from_rgb(238, 238, 238);
    p.text_dim = Color32::from_rgb(212, 212, 212);
    p.text_faint = Color32::from_rgb(160, 160, 160);
    p.hover = Some(Color32::from_rgb(98, 98, 98));
    p.pressed = Some(Color32::from_rgb(112, 112, 112));
    p.separator = Some(Color32::from_rgb(62, 62, 62));
    p.icon = Some(Color32::from_rgb(226, 226, 226));
    p.row_selected = Some(Color32::from_rgb(52, 98, 150));
    p
}

const fn pro_light() -> Palette {
    let mut p = Palette::new(
        false,
        [
            Color32::from_rgb(226, 226, 226),
            Color32::from_rgb(250, 250, 250),
            Color32::from_rgb(240, 240, 240),
            Color32::WHITE,
            Color32::from_rgb(186, 186, 186),
        ],
        [
            Color32::from_rgb(20, 20, 20),
            Color32::from_rgb(64, 64, 64),
            Color32::from_rgb(120, 120, 120),
        ],
        Color32::from_rgb(16, 110, 220),
        [
            Color32::from_rgb(200, 50, 60),
            Color32::from_rgb(30, 140, 70),
            Color32::from_rgb(180, 120, 10),
            Color32::from_rgb(20, 110, 220),
            Color32::from_rgb(150, 60, 190),
            Color32::from_rgb(0, 150, 160),
            Color32::from_rgb(220, 80, 40),
        ],
    );
    p.hover = Some(Color32::from_rgb(226, 226, 226));
    p.pressed = Some(Color32::from_rgb(206, 206, 206));
    p.separator = Some(Color32::from_rgb(214, 214, 214));
    p.icon = Some(Color32::from_rgb(46, 46, 46));
    p.accent_text = Some(Color32::WHITE);
    p.row_selected = Some(Color32::from_rgb(198, 220, 246));
    p.diff_add_bg = Some(Color32::from_rgba_unmultiplied_const(30, 160, 70, 40));
    p.diff_del_bg = Some(Color32::from_rgba_unmultiplied_const(220, 50, 60, 36));
    p.lanes = Some(CLASSIC_LIGHT_LANES);
    p.ansi = Some(PRO_LIGHT_ANSI);
    p
}

// Surfaces: tab strip, dock (behind the graph), chrome (toolbars and panels), field, border.
// Text: text, dim, faint. Hues: red, green, yellow, blue, magenta, cyan, orange.

mod palettes;
use palettes::*;

pub const BUILTIN: &[Builtin] = &[
    DARK,
    MEDIUMGRAY,
    LIGHT,
    CATPPUCCIN_MOCHA,
    CATPPUCCIN_MACCHIATO,
    CATPPUCCIN_FRAPPE,
    CATPPUCCIN_LATTE,
    TOKYO_NIGHT,
    TOKYO_NIGHT_STORM,
    TOKYO_NIGHT_DAY,
    ROSE_PINE,
    ROSE_PINE_MOON,
    ROSE_PINE_DAWN,
    KANAGAWA_WAVE,
    KANAGAWA_DRAGON,
    KANAGAWA_LOTUS,
    EVERFOREST_DARK,
    EVERFOREST_LIGHT,
    DRACULA,
    NORD,
    GRUVBOX_DARK,
    GRUVBOX_LIGHT,
    SOLARIZED_DARK,
    SOLARIZED_LIGHT,
    ONE_DARK,
    ONE_LIGHT,
    GITHUB_DARK,
    GITHUB_LIGHT,
    AYU_DARK,
    AYU_MIRAGE,
    AYU_LIGHT,
    MONOKAI,
    MONOKAI_PRO,
    NIGHT_OWL,
    PALENIGHT,
    TOMORROW_NIGHT,
    HORIZON,
    SYNTHWAVE_84,
    HIGH_CONTRAST_DARK,
    HIGH_CONTRAST_LIGHT,
];

#[cfg(test)]
mod tests;
