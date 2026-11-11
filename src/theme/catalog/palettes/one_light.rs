//! Published one_light palette values.
use super::super::*;

pub(in super::super) const ONE_LIGHT: Builtin = Builtin {
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
};
