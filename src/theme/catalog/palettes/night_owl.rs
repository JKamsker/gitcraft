//! Published night_owl palette values.
use super::super::*;

pub(in super::super) const NIGHT_OWL: Builtin = Builtin {
    id: "night-owl",
    name: "Night Owl",
    credit: "Night Owl · Sarah Drasner",
    palette: pal(
        true,
        [0x010e17, 0x011627, 0x01111d, 0x0b253a, 0x1d3b53],
        [0xd6deeb, 0xa7bbc7, 0x5f7e97],
        0x82aaff,
        [
            0xef5350, 0x22da6e, 0xecc48d, 0x82aaff, 0xc792ea, 0x7fdbca, 0xf78c6c,
        ],
    )
    .with_selection(hex(0x1d3b53)),
};
