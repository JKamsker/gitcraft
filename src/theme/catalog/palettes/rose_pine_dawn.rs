//! Published rose_pine_dawn palette values.
use super::super::*;

pub(in super::super) const ROSE_PINE_DAWN: Builtin = Builtin {
    id: "rose-pine-dawn",
    name: "Rosé Pine Dawn",
    credit: "Rosé Pine · rosepinetheme.com",
    palette: pal(
        false,
        [0xf2e9e1, 0xfaf4ed, 0xf4ede8, 0xfffaf3, 0xcecacd],
        [0x464261, 0x797593, 0x9893a5],
        0x286983,
        [
            0xb4637a, 0x56949f, 0xea9d34, 0x286983, 0x907aa9, 0x56949f, 0xd7827e,
        ],
    )
    .with_selection(hex(0xdfdad9)),
};
