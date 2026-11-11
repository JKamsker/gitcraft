//! Published kanagawa_dragon palette values.
use super::super::*;

pub(in super::super) const KANAGAWA_DRAGON: Builtin = Builtin {
    id: "kanagawa-dragon",
    name: "Kanagawa Dragon",
    credit: "Kanagawa · rebelot",
    palette: pal(
        true,
        [0x0d0c0c, 0x181616, 0x12120f, 0x0d0c0c, 0x625e5a],
        [0xc5c9c5, 0xa6a69c, 0x737c73],
        0x8ba4b0,
        [
            0xc4746e, 0x8a9a7b, 0xc4b28a, 0x8ba4b0, 0xa292a3, 0x8ea4a2, 0xb6927b,
        ],
    )
    .with_selection(hex(0x2d4f67)),
};
