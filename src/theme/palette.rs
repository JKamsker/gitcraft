//! A theme's colours as a theme is written: a few surfaces, three shades of text, an accent and
//! the usual terminal hues. Everything else the app paints with (hovers, selections, diff
//! backgrounds, lane and terminal colours) follows from them unless the theme sets it too.

use egui::Color32;

use super::Tokens;

/// `0xRRGGBB` as a colour, for the catalogue.
pub(crate) const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// The window's tab strip and panel headers.
    pub tab_strip: Color32,
    /// Behind the graph.
    pub dock: Color32,
    /// Toolbars, side panels, dialogs.
    pub chrome: Color32,
    /// Text fields and code blocks.
    pub field: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub red: Color32,
    pub green: Color32,
    pub yellow: Color32,
    pub blue: Color32,
    pub magenta: Color32,
    pub cyan: Color32,
    pub orange: Color32,
    /// What would otherwise be worked out from the colours above.
    pub hover: Option<Color32>,
    pub pressed: Option<Color32>,
    pub separator: Option<Color32>,
    pub icon: Option<Color32>,
    pub accent_text: Option<Color32>,
    /// Accent used as text on panels, separately from the filled button colour.
    pub accent_fg: Option<Color32>,
    pub row_selected: Option<Color32>,
    pub diff_add_bg: Option<Color32>,
    pub diff_del_bg: Option<Color32>,
    pub shadow: Option<Color32>,
    pub danger: Option<Color32>,
    pub warning: Option<Color32>,
    pub success: Option<Color32>,
    /// The graph's lanes, in order.
    pub lanes: Option<[Color32; 10]>,
    /// The terminal's 16 colours: black, red, green, yellow, blue, magenta, cyan, white, then
    /// their bright versions.
    pub ansi: Option<[Color32; 16]>,
}

/// The keys of a theme file's colours, as [`Palette::get`] and [`Palette::set`] know them (a
/// file's `lanes` and `ansi` are lists of colours beside them).
pub const KEYS: [&str; 29] = [
    "tab_strip",
    "dock",
    "chrome",
    "field",
    "border",
    "text",
    "text_dim",
    "text_faint",
    "accent",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "orange",
    "hover",
    "pressed",
    "separator",
    "icon",
    "accent_text",
    "accent_fg",
    "row_selected",
    "diff_add_bg",
    "diff_del_bg",
    "shadow",
    "danger",
    "warning",
    "success",
];

impl Palette {
    /// A palette of only the colours every theme has; the rest is worked out.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        dark: bool,
        [tab_strip, dock, chrome, field, border]: [Color32; 5],
        [text, text_dim, text_faint]: [Color32; 3],
        accent: Color32,
        [red, green, yellow, blue, magenta, cyan, orange]: [Color32; 7],
    ) -> Self {
        Self {
            dark,
            tab_strip,
            dock,
            chrome,
            field,
            border,
            text,
            text_dim,
            text_faint,
            accent,
            red,
            green,
            yellow,
            blue,
            magenta,
            cyan,
            orange,
            hover: None,
            pressed: None,
            separator: None,
            icon: None,
            accent_text: None,
            accent_fg: None,
            row_selected: None,
            diff_add_bg: None,
            diff_del_bg: None,
            shadow: None,
            danger: None,
            warning: None,
            success: None,
            lanes: None,
            ansi: None,
        }
    }

    pub const fn with_ansi(mut self, ansi: [Color32; 16]) -> Self {
        self.ansi = Some(ansi);
        self
    }

    pub const fn with_separator(mut self, c: Color32) -> Self {
        self.separator = Some(c);
        self
    }

    pub const fn with_selection(mut self, c: Color32) -> Self {
        self.row_selected = Some(c);
        self
    }

    /// The colour of `key` (one of [`KEYS`]), as the theme has it or as worked out.
    pub fn get(&self, key: &str) -> Option<Color32> {
        let t = self.tokens();
        Some(match key {
            "tab_strip" => self.tab_strip,
            "dock" => self.dock,
            "chrome" => self.chrome,
            "field" => self.field,
            "border" => self.border,
            "text" => self.text,
            "text_dim" => self.text_dim,
            "text_faint" => self.text_faint,
            "accent" => self.accent,
            "red" => self.red,
            "green" => self.green,
            "yellow" => self.yellow,
            "blue" => self.blue,
            "magenta" => self.magenta,
            "cyan" => self.cyan,
            "orange" => self.orange,
            "hover" => t.hover,
            "pressed" => t.pressed,
            "separator" => t.separator,
            "icon" => t.icon,
            "accent_text" => t.accent_text,
            "accent_fg" => t.accent_fg,
            "row_selected" => t.row_selected,
            "diff_add_bg" => t.diff_add_bg,
            "diff_del_bg" => t.diff_del_bg,
            "shadow" => t.shadow,
            "danger" => t.danger,
            "warning" => t.warning,
            "success" => t.success,
            _ => return None,
        })
    }

    /// Sets the colour of `key`; false for a key there is no such colour of.
    pub fn set(&mut self, key: &str, c: Color32) -> bool {
        let slot = match key {
            "tab_strip" => &mut self.tab_strip,
            "dock" => &mut self.dock,
            "chrome" => &mut self.chrome,
            "field" => &mut self.field,
            "border" => &mut self.border,
            "text" => &mut self.text,
            "text_dim" => &mut self.text_dim,
            "text_faint" => &mut self.text_faint,
            "accent" => {
                self.accent_fg = None;
                &mut self.accent
            }
            "red" => &mut self.red,
            "green" => &mut self.green,
            "yellow" => &mut self.yellow,
            "blue" => &mut self.blue,
            "magenta" => &mut self.magenta,
            "cyan" => &mut self.cyan,
            "orange" => &mut self.orange,
            other => {
                let opt = match other {
                    "hover" => &mut self.hover,
                    "pressed" => &mut self.pressed,
                    "separator" => &mut self.separator,
                    "icon" => &mut self.icon,
                    "accent_text" => &mut self.accent_text,
                    "accent_fg" => &mut self.accent_fg,
                    "row_selected" => &mut self.row_selected,
                    "diff_add_bg" => &mut self.diff_add_bg,
                    "diff_del_bg" => &mut self.diff_del_bg,
                    "shadow" => &mut self.shadow,
                    "danger" => &mut self.danger,
                    "warning" => &mut self.warning,
                    "success" => &mut self.success,
                    _ => return false,
                };
                *opt = Some(c);
                return true;
            }
        };
        *slot = c;
        true
    }

    /// The colours worked out from the theme's own, the theme's where it sets them.
    pub fn tokens(&self) -> Tokens {
        let dark = self.dark;
        let mix = |a: Color32, b: Color32, t: f32| a.lerp_to_gamma(b, t);
        let hover = self
            .hover
            .unwrap_or_else(|| mix(self.chrome, self.text, if dark { 0.09 } else { 0.07 }));
        let pressed = self
            .pressed
            .unwrap_or_else(|| mix(self.chrome, self.text, if dark { 0.16 } else { 0.14 }));
        let separator = self.separator.unwrap_or_else(|| {
            if dark {
                mix(self.tab_strip, Color32::BLACK, 0.2)
            } else {
                mix(self.chrome, self.text, 0.12)
            }
        });
        let accent_text = self.accent_text.unwrap_or_else(|| readable_on(self.accent));
        let row_selected = self
            .row_selected
            .unwrap_or_else(|| mix(self.dock, self.accent, if dark { 0.3 } else { 0.22 }));
        let translucent =
            |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
        let lanes = self.lanes.unwrap_or_else(|| self.derived_lanes());
        let ansi = self.ansi.unwrap_or_else(|| self.derived_ansi());
        Tokens {
            dark,
            chrome: self.chrome,
            dock: self.dock,
            card: self.chrome,
            field: self.field,
            field_border: self.border,
            hover,
            pressed,
            text: self.text,
            text_dim: self.text_dim,
            text_faint: self.text_faint,
            icon: self
                .icon
                .unwrap_or_else(|| mix(self.text, self.text_dim, 0.5)),
            accent: self.accent,
            accent_text,
            accent_fg: self.accent_fg.unwrap_or(self.accent),
            separator,
            shadow: self.shadow.unwrap_or(if dark {
                Color32::from_black_alpha(150)
            } else {
                Color32::from_black_alpha(50)
            }),
            danger: self.danger.unwrap_or(self.red),
            warning: self.warning.unwrap_or(self.yellow),
            success: self.success.unwrap_or(self.green),
            tab_strip: self.tab_strip,
            row_selected,
            diff_add_bg: self
                .diff_add_bg
                .unwrap_or_else(|| translucent(self.green, if dark { 34 } else { 40 })),
            diff_del_bg: self
                .diff_del_bg
                .unwrap_or_else(|| translucent(self.red, if dark { 34 } else { 36 })),
            lanes,
            ansi,
            radius_sm: 3.0,
            radius: 4.0,
            radius_lg: 6.0,
        }
    }

    /// Ten lanes from the theme's hues: the six of the terminal and four between them.
    fn derived_lanes(&self) -> [Color32; 10] {
        let mix = |a: Color32, b: Color32| a.lerp_to_gamma(b, 0.5);
        [
            self.blue,
            self.orange,
            self.green,
            self.magenta,
            self.yellow,
            self.cyan,
            self.red,
            mix(self.green, self.yellow),
            mix(self.blue, self.magenta),
            mix(self.orange, self.red),
        ]
    }

    /// The terminal's colours from the theme's hues: the bright ones a little lighter (darker
    /// on a light theme).
    fn derived_ansi(&self) -> [Color32; 16] {
        let toward = if self.dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        let bright = |c: Color32| c.lerp_to_gamma(toward, 0.18);
        let (black, white) = if self.dark {
            (self.tab_strip, self.text_dim)
        } else {
            (self.text, self.tab_strip)
        };
        let hues = [
            black,
            self.red,
            self.green,
            self.yellow,
            self.blue,
            self.magenta,
            self.cyan,
            white,
        ];
        let mut ansi = [Color32::BLACK; 16];
        for (i, c) in hues.iter().enumerate() {
            ansi[i] = *c;
            ansi[i + 8] = bright(*c);
        }
        ansi[8] = self.text_faint;
        ansi[15] = if self.dark { self.text } else { self.border };
        ansi
    }
}

/// How light `c` looks, 0 to 1 (WCAG's relative luminance).
pub fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG's contrast ratio of two colours, 1 to 21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Move only as far toward white or black as needed, preserving the foreground's hue.
pub(crate) fn aa_foreground(color: Color32, backgrounds: &[Color32]) -> Color32 {
    let minimum = |fg| {
        backgrounds
            .iter()
            .map(|bg| contrast(fg, *bg))
            .fold(21.0, f32::min)
    };
    if minimum(color) >= 4.5 {
        return color;
    }
    let toward = if minimum(Color32::WHITE) >= minimum(Color32::BLACK) {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    let (mut low, mut high) = (1_u16, 255_u16);
    while low < high {
        let mid = (low + high) / 2;
        if minimum(color.lerp_to_gamma(toward, mid as f32 / 255.0)) >= 4.5 {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    color.lerp_to_gamma(toward, high as f32 / 255.0)
}

/// Branch pills keep white labels; darken their lane shade enough for small text.
pub(crate) fn white_text_fill(color: Color32) -> Color32 {
    aa_foreground(color, &[Color32::WHITE])
}

/// The strongest requested tint that leaves its labels readable after compositing.
pub(crate) fn readable_tint(
    color: Color32,
    opacity: f32,
    backgrounds: &[Color32],
    foregrounds: &[Color32],
) -> Color32 {
    let tint = |alpha: u8| color.gamma_multiply(alpha as f32 / 255.0);
    let readable = |alpha, minimum| {
        backgrounds.iter().all(|bg| {
            let bg = over(tint(alpha), *bg);
            foregrounds.iter().all(|fg| contrast(*fg, bg) >= minimum)
        })
    };
    let requested = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    // Custom palettes that already fall below AA retain their chosen treatment.
    if !readable(0, 4.5) {
        return tint(requested);
    }
    if !readable(0, 4.55) {
        return Color32::TRANSPARENT;
    }
    let (mut low, mut high) = (0_u16, requested as u16);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if readable(mid as u8, 4.55) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    tint(low as u8)
}

/// White or near-black, whichever reads better on `bg`.
pub fn readable_on(bg: Color32) -> Color32 {
    let ink = Color32::from_rgb(22, 22, 26);
    if contrast(bg, Color32::WHITE) >= contrast(bg, ink) {
        Color32::WHITE
    } else {
        ink
    }
}

/// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (the `#` optional), as a colour.
pub fn parse_hex(text: &str) -> Option<Color32> {
    let s = text.trim().trim_start_matches('#');
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    let nibble = |i: usize| {
        u8::from_str_radix(s.get(i..i + 1)?, 16)
            .ok()
            .map(|v| v * 17)
    };
    let (r, g, b, a) = match s.len() {
        3 => (nibble(0)?, nibble(1)?, nibble(2)?, 255),
        4 => (nibble(0)?, nibble(1)?, nibble(2)?, nibble(3)?),
        6 => (byte(0)?, byte(2)?, byte(4)?, 255),
        8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        _ => return None,
    };
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// `#rrggbb`, or `#rrggbbaa` for a translucent colour.
pub fn to_hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// `c` laid over `bg`, opaque: a translucent colour from a theme file, where an opaque one is
/// needed.
pub fn over(c: Color32, bg: Color32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        return c;
    }
    let t = a as f32 / 255.0;
    let ch = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
    Color32::from_rgb(ch(r, bg.r()), ch(g, bg.g()), ch(b, bg.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_read_in_every_length() {
        assert_eq!(parse_hex("#fff"), Some(Color32::WHITE));
        assert_eq!(parse_hex("1e1e2e"), Some(hex(0x1e1e2e)));
        assert_eq!(
            parse_hex("#ff000080"),
            Some(Color32::from_rgba_unmultiplied(255, 0, 0, 128))
        );
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(to_hex(hex(0x89b4fa)), "#89b4fa");
    }

    #[test]
    fn accent_text_is_whichever_reads_better() {
        assert_eq!(readable_on(hex(0x1f6feb)), Color32::WHITE);
        assert_ne!(readable_on(hex(0xf1fa8c)), Color32::WHITE);
    }

    #[test]
    fn translucent_colours_are_laid_over_their_background() {
        let half_white = Color32::from_rgba_unmultiplied(255, 255, 255, 128);
        let c = over(half_white, Color32::BLACK);
        assert!((127..=129).contains(&c.r()));
        assert_eq!(over(Color32::RED, Color32::BLACK), Color32::RED);
    }
}
