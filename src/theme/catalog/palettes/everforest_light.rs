//! Published everforest_light palette values.
use super::super::*;

pub(in super::super) const EVERFOREST_LIGHT: Builtin = Builtin {
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
};
