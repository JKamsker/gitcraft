//! Published high_contrast_dark palette values.
use super::super::*;

pub(in super::super) const HIGH_CONTRAST_DARK: Builtin = Builtin {
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
};
