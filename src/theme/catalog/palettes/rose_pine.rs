//! Published rose_pine palette values.
use super::super::*;

pub(in super::super) const ROSE_PINE: Builtin = Builtin {
    id: "rose-pine",
    name: "Rosé Pine",
    credit: "Rosé Pine · rosepinetheme.com",
    palette: pal(
        true,
        [0x13111d, 0x191724, 0x1f1d2e, 0x26233a, 0x403d52],
        [0xe0def4, 0x908caa, 0x6e6a86],
        0xc4a7e7,
        [
            0xeb6f92, 0x9ccfd8, 0xf6c177, 0x3e8fb0, 0xc4a7e7, 0x56949f, 0xebbcba,
        ],
    )
    .with_selection(hex(0x403d52)),
};
