//! Published kanagawa_wave palette values.
use super::super::*;

pub(in super::super) const KANAGAWA_WAVE: Builtin = Builtin {
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
};
