//! Published rose_pine_moon palette values.
use super::super::*;

pub(in super::super) const ROSE_PINE_MOON: Builtin = Builtin {
    id: "rose-pine-moon",
    name: "Rosé Pine Moon",
    credit: "Rosé Pine · rosepinetheme.com",
    palette: pal(
        true,
        [0x1c1a2b, 0x232136, 0x2a273f, 0x393552, 0x44415a],
        [0xe0def4, 0x908caa, 0x6e6a86],
        0xc4a7e7,
        [
            0xeb6f92, 0x9ccfd8, 0xf6c177, 0x3e8fb0, 0xc4a7e7, 0x56949f, 0xea9a97,
        ],
    )
    .with_selection(hex(0x44415a)),
};
