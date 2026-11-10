//! Published everforest_dark palette values.
use super::super::*;

pub(in super::super) const EVERFOREST_DARK: Builtin = Builtin {
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
};
