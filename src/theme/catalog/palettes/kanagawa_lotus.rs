//! Published kanagawa_lotus palette values.
use super::super::*;

pub(in super::super) const KANAGAWA_LOTUS: Builtin = Builtin {
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
};
