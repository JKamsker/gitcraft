//! Published monokai_pro palette values.
use super::super::*;

pub(in super::super) const MONOKAI_PRO: Builtin = Builtin {
    id: "monokai-pro",
    name: "Monokai Pro",
    credit: "Monokai Pro · monokai.pro",
    palette: pal(
        true,
        [0x19181a, 0x2d2a2e, 0x221f22, 0x403e41, 0x5b595c],
        [0xfcfcfa, 0xc1c0c0, 0x939293],
        0xffd866,
        [
            0xff6188, 0xa9dc76, 0xffd866, 0x78dce8, 0xab9df2, 0x78dce8, 0xfc9867,
        ],
    )
    .with_selection(hex(0x5b595c)),
};
