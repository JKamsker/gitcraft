//! Published high_contrast_light palette values.
use super::super::*;

pub(in super::super) const HIGH_CONTRAST_LIGHT: Builtin = Builtin {
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
};
