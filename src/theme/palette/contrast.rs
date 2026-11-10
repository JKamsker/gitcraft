//! Colour parsing, compositing, luminance, and readable contrast selection.
use super::*;

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
