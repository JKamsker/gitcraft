//! Published ayu_mirage palette values.
use super::super::*;

pub(in super::super) const AYU_MIRAGE: Builtin = Builtin {
    id: "ayu-mirage",
    name: "Ayu Mirage",
    credit: "Ayu · dempfi",
    palette: pal(
        true,
        [0x171b24, 0x1f2430, 0x1c212b, 0x191e2a, 0x33415e],
        [0xcccac2, 0x8a9199, 0x707a8c],
        0xffcc66,
        [
            0xf28779, 0x87d96c, 0xffd173, 0x73d0ff, 0xdfbfff, 0x5ccfe6, 0xffad66,
        ],
    )
    .with_selection(hex(0x33415e)),
};
