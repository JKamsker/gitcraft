//! Published ayu_dark palette values.
use super::super::*;

pub(in super::super) const AYU_DARK: Builtin = Builtin {
    id: "ayu-dark",
    name: "Ayu Dark",
    credit: "Ayu · dempfi",
    palette: pal(
        true,
        [0x0b0e14, 0x0d1017, 0x0f131a, 0x0b0e14, 0x2d3640],
        [0xbfbdb6, 0x8a8f98, 0x565b66],
        0xe6b450,
        [
            0xf07178, 0x7fd962, 0xffb454, 0x59c2ff, 0xd2a6ff, 0x39bae6, 0xff8f40,
        ],
    )
    .with_selection(hex(0x1b3a5b)),
};
