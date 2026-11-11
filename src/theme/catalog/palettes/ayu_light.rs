//! Published ayu_light palette values.
use super::super::*;

pub(in super::super) const AYU_LIGHT: Builtin = Builtin {
    id: "ayu-light",
    name: "Ayu Light",
    credit: "Ayu · dempfi",
    palette: pal(
        false,
        [0xebeef0, 0xfcfcfc, 0xf3f4f5, 0xffffff, 0xdcdfe3],
        [0x5c6166, 0x787b80, 0x8a9199],
        0xfa8d3e,
        [
            0xe65050, 0x6cbf43, 0xf2ae49, 0x399ee6, 0xa37acc, 0x55b4d4, 0xfa8d3e,
        ],
    )
    .with_selection(hex(0xd1e4f4)),
};
