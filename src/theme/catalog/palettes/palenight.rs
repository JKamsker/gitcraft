//! Published palenight palette values.
use super::super::*;

pub(in super::super) const PALENIGHT: Builtin = Builtin {
    id: "palenight",
    name: "Material Palenight",
    credit: "Material Theme · Mattia Astorino",
    palette: pal(
        true,
        [0x1b1e2b, 0x292d3e, 0x242839, 0x202331, 0x3a3f58],
        [0xbfc7d5, 0xa6accd, 0x676e95],
        0x82aaff,
        [
            0xff5370, 0xc3e88d, 0xffcb6b, 0x82aaff, 0xc792ea, 0x89ddff, 0xf78c6c,
        ],
    )
    .with_selection(hex(0x3c435e)),
};
