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

pub const BUILTIN: &[Builtin] = &[
    Builtin {
        id: "Dark",
        name: "gitcraft Dark",
        credit: "PhotoCraft's Pro (Dark)",
        palette: pro_dark(),
    },
    Builtin {
        id: "MediumGray",
        name: "gitcraft Medium Gray",
        credit: "PhotoCraft's Pro (Medium Gray)",
        palette: pro_medium_gray(),
    },
    Builtin {
        id: "Light",
        name: "gitcraft Light",
        credit: "PhotoCraft's Pro (Light)",
        palette: pro_light(),
    },
    // Catppuccin: github.com/catppuccin/palette
    Builtin {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        credit: "Catppuccin · catppuccin.com",
        palette: pal(
            true,
            [0x11111b, 0x1e1e2e, 0x181825, 0x11111b, 0x45475a],
            [0xcdd6f4, 0xa6adc8, 0x7f849c],
            0xcba6f7,
            [
                0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xfab387,
            ],
        )
        .with_separator(hex(0x11111b))
        .with_selection(hex(0x45475a)),
    },
    Builtin {
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        credit: "Catppuccin · catppuccin.com",
        palette: pal(
            true,
            [0x181926, 0x24273a, 0x1e2030, 0x181926, 0x494d64],
            [0xcad3f5, 0xa5adcb, 0x8087a2],
            0xc6a0f6,
            [
                0xed8796, 0xa6da95, 0xeed49f, 0x8aadf4, 0xf5bde6, 0x8bd5ca, 0xf5a97f,
            ],
        )
        .with_separator(hex(0x181926))
        .with_selection(hex(0x494d64)),
    },
    Builtin {
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        credit: "Catppuccin · catppuccin.com",
        palette: pal(
            true,
            [0x232634, 0x303446, 0x292c3c, 0x232634, 0x51576d],
            [0xc6d0f5, 0xa5adce, 0x838ba7],
            0xca9ee6,
            [
                0xe78284, 0xa6d189, 0xe5c890, 0x8caaee, 0xf4b8e4, 0x81c8be, 0xef9f76,
            ],
        )
        .with_separator(hex(0x232634))
        .with_selection(hex(0x51576d)),
    },
    Builtin {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        credit: "Catppuccin · catppuccin.com",
        palette: pal(
            false,
            [0xdce0e8, 0xeff1f5, 0xe6e9ef, 0xf7f8fa, 0xbcc0cc],
            [0x4c4f69, 0x5c5f77, 0x8c8fa1],
            0x8839ef,
            [
                0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xfe640b,
            ],
        )
        .with_selection(hex(0xccd0da)),
    },
    // Tokyo Night: github.com/folke/tokyonight.nvim
    Builtin {
        id: "tokyo-night",
        name: "Tokyo Night",
        credit: "Tokyo Night · enkia, folke",
        palette: pal(
            true,
            [0x0c0e14, 0x1a1b26, 0x16161e, 0x0c0e14, 0x3b4261],
            [0xc0caf5, 0xa9b1d6, 0x737aa2],
            0x7aa2f7,
            [
                0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xff9e64,
            ],
        )
        .with_selection(hex(0x283457)),
    },
    Builtin {
        id: "tokyo-night-storm",
        name: "Tokyo Night Storm",
        credit: "Tokyo Night · enkia, folke",
        palette: pal(
            true,
            [0x1b1e2d, 0x24283b, 0x1f2335, 0x1b1e2d, 0x3b4261],
            [0xc0caf5, 0xa9b1d6, 0x737aa2],
            0x7aa2f7,
            [
                0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xff9e64,
            ],
        )
        .with_selection(hex(0x2e3c64)),
    },
    Builtin {
        id: "tokyo-night-day",
        name: "Tokyo Night Day",
        credit: "Tokyo Night · enkia, folke",
        palette: pal(
            false,
            [0xd0d5e3, 0xe1e2e7, 0xe9e9ec, 0xf2f2f5, 0xa8aecb],
            [0x3760bf, 0x6172b0, 0x848cb5],
            0x2e7de9,
            [
                0xf52a65, 0x587539, 0x8c6c3e, 0x2e7de9, 0x9854f1, 0x007197, 0xb15c00,
            ],
        ),
    },
    // Rosé Pine: github.com/rose-pine/palette
    Builtin {
        id: "rose-pine",
        name: "Rosé Pine",
        credit: "Rosé Pine · rosepinetheme.com",
        palette: pal(
            true,
            [0x13111d, 0x191724, 0x1f1d2e, 0x26233a, 0x403d52],
            [0xe0def4, 0x908caa, 0x6e6a86],
            0xc4a7e7,
            [
                0xeb6f92, 0x9ccfd8, 0xf6c177, 0x3e8fb0, 0xc4a7e7, 0x56949f, 0xebbcba,
            ],
        )
        .with_selection(hex(0x403d52)),
    },
    Builtin {
        id: "rose-pine-moon",
        name: "Rosé Pine Moon",
        credit: "Rosé Pine · rosepinetheme.com",
        palette: pal(
            true,
            [0x1c1a2b, 0x232136, 0x2a273f, 0x393552, 0x44415a],
            [0xe0def4, 0x908caa, 0x6e6a86],
            0xc4a7e7,
            [
                0xeb6f92, 0x9ccfd8, 0xf6c177, 0x3e8fb0, 0xc4a7e7, 0x56949f, 0xea9a97,
            ],
        )
        .with_selection(hex(0x44415a)),
    },
    Builtin {
        id: "rose-pine-dawn",
        name: "Rosé Pine Dawn",
        credit: "Rosé Pine · rosepinetheme.com",
        palette: pal(
            false,
            [0xf2e9e1, 0xfaf4ed, 0xf4ede8, 0xfffaf3, 0xcecacd],
            [0x464261, 0x797593, 0x9893a5],
            0x286983,
            [
                0xb4637a, 0x56949f, 0xea9d34, 0x286983, 0x907aa9, 0x56949f, 0xd7827e,
            ],
        )
        .with_selection(hex(0xdfdad9)),
    },
    // Kanagawa: github.com/rebelot/kanagawa.nvim
    Builtin {
        id: "kanagawa-wave",
        name: "Kanagawa Wave",
        credit: "Kanagawa · rebelot",
        palette: pal(
            true,
            [0x16161d, 0x1f1f28, 0x1a1a22, 0x16161d, 0x54546d],
            [0xdcd7ba, 0xc8c093, 0x727169],
            0x7e9cd8,
            [
                0xe46876, 0x98bb6c, 0xe6c384, 0x7e9cd8, 0x957fb8, 0x7aa89f, 0xffa066,
            ],
        )
        .with_selection(hex(0x2d4f67)),
    },
    Builtin {
        id: "kanagawa-dragon",
        name: "Kanagawa Dragon",
        credit: "Kanagawa · rebelot",
        palette: pal(
            true,
            [0x0d0c0c, 0x181616, 0x12120f, 0x0d0c0c, 0x625e5a],
            [0xc5c9c5, 0xa6a69c, 0x737c73],
            0x8ba4b0,
            [
                0xc4746e, 0x8a9a7b, 0xc4b28a, 0x8ba4b0, 0xa292a3, 0x8ea4a2, 0xb6927b,
            ],
        )
        .with_selection(hex(0x2d4f67)),
    },
    Builtin {
        id: "kanagawa-lotus",
        name: "Kanagawa Lotus",
        credit: "Kanagawa · rebelot",
        palette: pal(
            false,
            [0xdcd5ac, 0xf2ecbc, 0xe5ddb0, 0xf8f3d0, 0xc8c093],
            [0x545464, 0x716e61, 0x8a8980],
            0x4d699b,
            [
                0xc84053, 0x6f894e, 0xde9800, 0x4d699b, 0xb35b79, 0x597b75, 0xcc6d00,
            ],
        )
        .with_selection(hex(0xc7d7e0)),
    },
    // Everforest: github.com/sainnhe/everforest
    Builtin {
        id: "everforest-dark",
        name: "Everforest Dark",
        credit: "Everforest · sainnhe",
        palette: pal(
            true,
            [0x232a2e, 0x2d353b, 0x343f44, 0x232a2e, 0x4f585e],
            [0xd3c6aa, 0x9da9a0, 0x859289],
            0xa7c080,
            [
                0xe67e80, 0xa7c080, 0xdbbc7f, 0x7fbbb3, 0xd699b6, 0x83c092, 0xe69875,
            ],
        )
        .with_selection(hex(0x475258)),
    },
    Builtin {
        id: "everforest-light",
        name: "Everforest Light",
        credit: "Everforest · sainnhe",
        palette: pal(
            false,
            [0xefebd4, 0xfdf6e3, 0xf4f0d9, 0xfffbef, 0xbdc3af],
            [0x5c6a72, 0x829181, 0x939f91],
            0x3a94c5,
            [
                0xf85552, 0x8da101, 0xdfa000, 0x3a94c5, 0xdf69ba, 0x35a77c, 0xf57d26,
            ],
        )
        .with_selection(hex(0xe6e2cc)),
    },
    // Dracula: draculatheme.com/contribute
    Builtin {
        id: "dracula",
        name: "Dracula",
        credit: "Dracula · draculatheme.com",
        palette: pal(
            true,
            [0x191a21, 0x282a36, 0x21222c, 0x191a21, 0x44475a],
            [0xf8f8f2, 0xc5c8d6, 0x7b88b8],
            0xbd93f9,
            [
                0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xffb86c,
            ],
        )
        .with_selection(hex(0x44475a))
        .with_ansi(ansi([
            0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
            0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
        ])),
    },
    // Nord: nordtheme.com/docs/colors-and-palettes
    Builtin {
        id: "nord",
        name: "Nord",
        credit: "Nord · Arctic Ice Studio",
        palette: pal(
            true,
            [0x242933, 0x2e3440, 0x3b4252, 0x2b303b, 0x4c566a],
            [0xeceff4, 0xd8dee9, 0x8590a6],
            0x88c0d0,
            [
                0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xd08770,
            ],
        )
        .with_selection(hex(0x434c5e))
        .with_ansi(ansi([
            0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
            0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
        ])),
    },
    // Gruvbox: github.com/morhetz/gruvbox
    Builtin {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        credit: "Gruvbox · morhetz",
        palette: pal(
            true,
            [0x1d2021, 0x282828, 0x32302f, 0x1d2021, 0x504945],
            [0xebdbb2, 0xd5c4a1, 0x928374],
            0x83a598,
            [
                0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xfe8019,
            ],
        )
        .with_selection(hex(0x504945)),
    },
    Builtin {
        id: "gruvbox-light",
        name: "Gruvbox Light",
        credit: "Gruvbox · morhetz",
        palette: pal(
            false,
            [0xebdbb2, 0xfbf1c7, 0xf2e5bc, 0xf9f5d7, 0xd5c4a1],
            [0x3c3836, 0x504945, 0x7c6f64],
            0x076678,
            [
                0x9d0006, 0x79740e, 0xb57614, 0x076678, 0x8f3f71, 0x427b58, 0xaf3a03,
            ],
        )
        .with_selection(hex(0xd5c4a1)),
    },
    // Solarized: ethanschoonover.com/solarized
    Builtin {
        id: "solarized-dark",
        name: "Solarized Dark",
        credit: "Solarized · Ethan Schoonover",
        palette: pal(
            true,
            [0x001e26, 0x002b36, 0x00212b, 0x073642, 0x1d4a54],
            [0x93a1a1, 0x839496, 0x586e75],
            0x268bd2,
            [
                0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xcb4b16,
            ],
        )
        .with_selection(hex(0x073642))
        .with_ansi(ansi([
            0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x586e75, 0xcb4b16, 0x859900, 0xb58900, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ])),
    },
    Builtin {
        id: "solarized-light",
        name: "Solarized Light",
        credit: "Solarized · Ethan Schoonover",
        palette: pal(
            false,
            [0xe4ddc8, 0xfdf6e3, 0xeee8d5, 0xfffdf6, 0xd3cbb7],
            [0x073642, 0x586e75, 0x839496],
            0x268bd2,
            [
                0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xcb4b16,
            ],
        )
        .with_selection(hex(0xe4ddc8)),
    },
    // One Dark / One Light: Atom's themes.
    Builtin {
        id: "one-dark",
        name: "One Dark",
        credit: "Atom One · GitHub",
        palette: pal(
            true,
            [0x1b1d23, 0x282c34, 0x21252b, 0x1d1f23, 0x3e4451],
            [0xd7dae0, 0xabb2bf, 0x7f848e],
            0x61afef,
            [
                0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd19a66,
            ],
        )
        .with_selection(hex(0x3e4451)),
    },
    Builtin {
        id: "one-light",
        name: "One Light",
        credit: "Atom One · GitHub",
        palette: pal(
            false,
            [0xe5e5e6, 0xfafafa, 0xf0f0f1, 0xffffff, 0xd4d4d6],
            [0x383a42, 0x696c77, 0xa0a1a7],
            0x4078f2,
            [
                0xe45649, 0x50a14f, 0xc18401, 0x4078f2, 0xa626a4, 0x0184bc, 0x986801,
            ],
        ),
    },
    // GitHub: github.com/primer/primitives
    Builtin {
        id: "github-dark",
        name: "GitHub Dark",
        credit: "GitHub Primer",
        palette: pal(
            true,
            [0x010409, 0x0d1117, 0x151b23, 0x0d1117, 0x3d444d],
            [0xf0f6fc, 0x9198a1, 0x7d8590],
            0x388bfd,
            [
                0xf85149, 0x3fb950, 0xd29922, 0x58a6ff, 0xbe8fff, 0x39c5cf, 0xf0883e,
            ],
        )
        .with_selection(hex(0x1f3a5f)),
    },
    Builtin {
        id: "github-light",
        name: "GitHub Light",
        credit: "GitHub Primer",
        palette: pal(
            false,
            [0xeaeef2, 0xffffff, 0xf6f8fa, 0xffffff, 0xd0d7de],
            [0x1f2328, 0x59636e, 0x818b98],
            0x0969da,
            [
                0xcf222e, 0x1a7f37, 0x9a6700, 0x0969da, 0x8250df, 0x1b7c83, 0xbc4c00,
            ],
        )
        .with_selection(hex(0xddf4ff)),
    },
    // Ayu: github.com/ayu-theme
    Builtin {
        id: "ayu-dark",
        name: "Ayu Dark",
        credit: "Ayu · dempfi",
        palette: pal(
            true,
            [0x0b0e14, 0x0d1017, 0x0f131a, 0x0b0e14, 0x2d3640],
            [0xbfbdb6, 0x8a8f98, 0x565b66],
            0xe6b450,
            [
                0xf07178, 0x7fd962, 0xffb454, 0x59c2ff, 0xd2a6ff, 0x39bae6, 0xff8f40,
            ],
        )
        .with_selection(hex(0x1b3a5b)),
    },
    Builtin {
        id: "ayu-mirage",
        name: "Ayu Mirage",
        credit: "Ayu · dempfi",
        palette: pal(
            true,
            [0x171b24, 0x1f2430, 0x1c212b, 0x191e2a, 0x33415e],
            [0xcccac2, 0x8a9199, 0x707a8c],
            0xffcc66,
            [
                0xf28779, 0x87d96c, 0xffd173, 0x73d0ff, 0xdfbfff, 0x5ccfe6, 0xffad66,
            ],
        )
        .with_selection(hex(0x33415e)),
    },
    Builtin {
        id: "ayu-light",
        name: "Ayu Light",
        credit: "Ayu · dempfi",
        palette: pal(
            false,
            [0xebeef0, 0xfcfcfc, 0xf3f4f5, 0xffffff, 0xdcdfe3],
            [0x5c6166, 0x787b80, 0x8a9199],
            0xfa8d3e,
            [
                0xe65050, 0x6cbf43, 0xf2ae49, 0x399ee6, 0xa37acc, 0x55b4d4, 0xfa8d3e,
            ],
        )
        .with_selection(hex(0xd1e4f4)),
    },
    // Monokai: Wimer Hazenberg; Monokai Pro: monokai.pro
    Builtin {
        id: "monokai",
        name: "Monokai",
        credit: "Monokai · Wimer Hazenberg",
        palette: pal(
            true,
            [0x191a16, 0x272822, 0x1e1f1c, 0x31322a, 0x49483e],
            [0xf8f8f2, 0xcfcfc2, 0x90908a],
            0x66d9ef,
            [
                0xf92672, 0xa6e22e, 0xe6db74, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xfd971f,
            ],
        )
        .with_selection(hex(0x49483e)),
    },
    Builtin {
        id: "monokai-pro",
        name: "Monokai Pro",
        credit: "Monokai Pro · monokai.pro",
        palette: pal(
            true,
            [0x19181a, 0x2d2a2e, 0x221f22, 0x403e41, 0x5b595c],
            [0xfcfcfa, 0xc1c0c0, 0x939293],
            0xffd866,
            [
                0xff6188, 0xa9dc76, 0xffd866, 0x78dce8, 0xab9df2, 0x78dce8, 0xfc9867,
            ],
        )
        .with_selection(hex(0x5b595c)),
    },
    // Night Owl: github.com/sdras/night-owl-vscode-theme
    Builtin {
        id: "night-owl",
        name: "Night Owl",
        credit: "Night Owl · Sarah Drasner",
        palette: pal(
            true,
            [0x010e17, 0x011627, 0x01111d, 0x0b253a, 0x1d3b53],
            [0xd6deeb, 0xa7bbc7, 0x5f7e97],
            0x82aaff,
            [
                0xef5350, 0x22da6e, 0xecc48d, 0x82aaff, 0xc792ea, 0x7fdbca, 0xf78c6c,
            ],
        )
        .with_selection(hex(0x1d3b53)),
    },
    // Material Palenight: material-theme.com
    Builtin {
        id: "palenight",
        name: "Material Palenight",
        credit: "Material Theme · Mattia Astorino",
        palette: pal(
            true,
            [0x1b1e2b, 0x292d3e, 0x242839, 0x202331, 0x3a3f58],
            [0xbfc7d5, 0xa6accd, 0x676e95],
            0x82aaff,
            [
                0xff5370, 0xc3e88d, 0xffcb6b, 0x82aaff, 0xc792ea, 0x89ddff, 0xf78c6c,
            ],
        )
        .with_selection(hex(0x3c435e)),
    },
    // Tomorrow Night: github.com/chriskempson/tomorrow-theme
    Builtin {
        id: "tomorrow-night",
        name: "Tomorrow Night",
        credit: "Tomorrow · Chris Kempson",
        palette: pal(
            true,
            [0x161719, 0x1d1f21, 0x232528, 0x161719, 0x373b41],
            [0xc5c8c6, 0xb4b7b4, 0x969896],
            0x81a2be,
            [
                0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xde935f,
            ],
        )
        .with_selection(hex(0x373b41)),
    },
    // Horizon: github.com/jolaleye/horizon-theme-vscode
    Builtin {
        id: "horizon",
        name: "Horizon",
        credit: "Horizon · jolaleye",
        palette: pal(
            true,
            [0x16161c, 0x1c1e26, 0x232530, 0x1a1c23, 0x2e303e],
            [0xd5d8da, 0xbbbbbb, 0x6c6f93],
            0xe95678,
            [
                0xe95678, 0x29d398, 0xfab795, 0x26bbd9, 0xee64ac, 0x59e3e3, 0xf09483,
            ],
        )
        .with_selection(hex(0x2e303e)),
    },
    // SynthWave '84: github.com/robb0wen/synthwave-vscode
    Builtin {
        id: "synthwave-84",
        name: "SynthWave '84",
        credit: "SynthWave '84 · Robb Owen",
        palette: pal(
            true,
            [0x171520, 0x262335, 0x241b2f, 0x2a2139, 0x495495],
            [0xffffff, 0xe0dbe8, 0x848bbd],
            0xff7edb,
            [
                0xfe4450, 0x72f1b8, 0xfede5d, 0x2ee2fa, 0xff7edb, 0x36f9f6, 0xf97e72,
            ],
        )
        .with_selection(hex(0x463465)),
    },
    // High contrast, for low vision or bright rooms.
    Builtin {
        id: "high-contrast-dark",
        name: "High Contrast Dark",
        credit: "gitcraft",
        palette: {
            let mut p = pal(
                true,
                [0x000000, 0x000000, 0x0a0a0a, 0x000000, 0x9a9a9a],
                [0xffffff, 0xeeeeee, 0xc4c4c4],
                0x4cc2ff,
                [
                    0xff6b6b, 0x3fe07a, 0xffd700, 0x4cc2ff, 0xff7af0, 0x3ff2f2, 0xff9f40,
                ],
            )
            .with_separator(hex(0x6f6f6f))
            .with_selection(hex(0x00426b));
            p.hover = Some(hex(0x2a2a2a));
            p.pressed = Some(hex(0x3d3d3d));
            p
        },
    },
    Builtin {
        id: "high-contrast-light",
        name: "High Contrast Light",
        credit: "gitcraft",
        palette: pal(
            false,
            [0xe8e8e8, 0xffffff, 0xf5f5f5, 0xffffff, 0x5a5a5a],
            [0x000000, 0x1e1e1e, 0x4a4a4a],
            0x0f4a85,
            [
                0xb5200d, 0x136c13, 0x8a5a00, 0x0f4a85, 0x8b008b, 0x006b75, 0xa33f00,
            ],
        )
        .with_separator(hex(0x8a8a8a))
        .with_selection(hex(0xbfdcf5)),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::palette::contrast;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = BUILTIN.iter().map(|b| b.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn every_builtin_small_text_token_meets_wcag_aa_on_its_surfaces() {
        let mut unreadable = Vec::new();
        for b in BUILTIN {
            let t = b.palette().tokens();
            let mut check = |what: &str, fg, surface: &str, bg| {
                let ratio = contrast(fg, bg);
                if ratio < 4.5 {
                    unreadable.push(format!("{}: {what} on {surface} is {ratio:.2}:1", b.name));
                }
            };
            let surfaces = [
                ("tab strip", t.tab_strip),
                ("dock", t.dock),
                ("card", t.card),
                ("field", t.field),
                ("hover", t.hover),
                ("pressed", t.pressed),
                ("selected row", t.row_selected),
            ];
            for (name, fg) in [
                ("text", t.text),
                ("dim text", t.text_dim),
                ("faint text", t.text_faint),
                ("accent foreground", t.accent_fg),
                ("danger", t.danger),
                ("warning", t.warning),
                ("success", t.success),
            ] {
                for (surface, bg) in surfaces {
                    if name == "text" || name == "dim text" || surface != "pressed" {
                        check(name, fg, surface, bg);
                    }
                }
            }
            for (name, fg) in [
                ("accent chip", t.accent_fg),
                ("danger chip", t.danger),
                ("warning chip", t.warning),
                ("success chip", t.success),
            ] {
                for (surface, bg) in surfaces.iter().filter(|(name, _)| *name != "pressed") {
                    for opacity in [0.18, 0.22] {
                        check(name, fg, surface, over(t.text_tint(fg, opacity, *bg), *bg));
                    }
                }
            }
            check("button label", t.accent_text, "accent", t.accent);
            check(
                "diff hunk label",
                t.accent_fg,
                "hunk fill",
                over(t.accent.gamma_multiply(0.12), t.field),
            );
            for (name, fill) in [
                ("added line", t.diff_add_bg),
                ("deleted line", t.diff_del_bg),
            ] {
                check("diff text", t.text, name, over(fill, t.field));
                let added = name == "added line";
                for surface in [t.field, t.card] {
                    let line = over(fill, surface);
                    for bg in [line, over(t.accent.gamma_multiply(0.22), line)] {
                        check(
                            "intraline text",
                            t.text,
                            name,
                            over(t.diff_strong_bg(added), bg),
                        );
                    }
                }
            }
        }
        assert!(unreadable.is_empty(), "{unreadable:#?}");
    }

    #[test]
    fn contrast_changes_preserve_surfaces_terminal_and_graph_colours() {
        for b in BUILTIN {
            let original = b.palette.tokens();
            let adjusted = b.palette().tokens();
            assert_eq!(adjusted.tab_strip, original.tab_strip, "{}", b.name);
            assert_eq!(adjusted.dock, original.dock, "{}", b.name);
            assert_eq!(adjusted.chrome, original.chrome, "{}", b.name);
            assert_eq!(adjusted.field, original.field, "{}", b.name);
            assert_eq!(adjusted.hover, original.hover, "{}", b.name);
            assert_eq!(adjusted.pressed, original.pressed, "{}", b.name);
            assert_eq!(adjusted.row_selected, original.row_selected, "{}", b.name);
            assert_eq!(adjusted.lanes, original.lanes, "{}", b.name);
            assert_eq!(adjusted.ansi, original.ansi, "{}", b.name);
        }
        for b in &BUILTIN[..3] {
            let t = b.palette().tokens();
            assert_eq!(t.accent_text, Color32::WHITE);
            assert!(contrast(t.accent_text, t.accent) >= 4.5);
        }
    }

    #[test]
    fn a_custom_palette_keeps_its_chosen_text_colours() {
        let mut p = BUILTIN[0].palette();
        p.text_faint = p.chrome;
        p.accent_fg = None;
        let t = p.tokens();
        assert_eq!(t.text_faint, p.chrome);
        assert_eq!(t.accent_fg, p.accent);
    }

    #[test]
    fn the_pro_themes_keep_their_colours() {
        let dark = BUILTIN[0].palette().tokens();
        assert_eq!(dark.chrome, Color32::from_rgb(50, 50, 50));
        assert_eq!(dark.hover, Color32::from_rgb(66, 66, 66));
        assert_eq!(dark.row_selected, Color32::from_rgb(38, 76, 122));
        assert_eq!(dark.lanes, CLASSIC_DARK_LANES);
        let light = BUILTIN[2].palette().tokens();
        assert!(!light.dark);
        assert_eq!(light.field, Color32::WHITE);
        assert_eq!(light.separator, Color32::from_rgb(214, 214, 214));
    }
}
