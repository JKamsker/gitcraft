//! The terminal's colours: the 16 ANSI ones of the theme, the 256-colour cube and true colour.

use egui::Color32;

use crate::theme::Tokens;

/// Terminal colours: the theme's.
pub(super) struct Palette {
    fg: Color32,
    ansi: [Color32; 16],
}

impl Palette {
    pub(super) fn new(t: &Tokens) -> Self {
        let ansi = t.ansi;
        Self { fg: t.text, ansi }
    }

    pub(super) fn color(&self, c: vt100::Color) -> Option<Color32> {
        match c {
            vt100::Color::Default => None,
            vt100::Color::Idx(i) if i < 16 => self.ansi.get(i as usize).copied(),
            vt100::Color::Idx(i) if i < 232 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Some(Color32::from_rgb(
                    level(i / 36),
                    level((i / 6) % 6),
                    level(i % 6),
                ))
            }
            vt100::Color::Idx(i) => {
                let v = 8 + (i - 232).saturating_mul(10);
                Some(Color32::from_gray(v))
            }
            vt100::Color::Rgb(r, g, b) => Some(Color32::from_rgb(r, g, b)),
        }
    }

    /// (foreground, background-if-not-default) of a cell.
    pub(super) fn colors(&self, cell: &vt100::Cell) -> (Color32, Option<Color32>) {
        let mut fg = self.color(cell.fgcolor()).unwrap_or(self.fg);
        let mut bg = self.color(cell.bgcolor());
        if cell.bold()
            && let vt100::Color::Idx(i) = cell.fgcolor()
            && i < 8
        {
            fg = self.ansi.get(i as usize + 8).copied().unwrap_or(fg);
        }
        if cell.dim() {
            fg = fg.gamma_multiply(0.6);
        }
        if cell.inverse() {
            let old_fg = fg;
            fg = bg.unwrap_or(Color32::from_rgb(30, 30, 30));
            bg = Some(old_fg);
        }
        (fg, bg)
    }
}
