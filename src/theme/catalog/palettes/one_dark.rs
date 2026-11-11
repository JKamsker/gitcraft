//! Published one_dark palette values.
use super::super::*;

pub(in super::super) const ONE_DARK: Builtin = Builtin {
    id: "one-dark",
    name: "One Dark",
    credit: "Atom One · GitHub",
    palette: pal(
        true,
        [0x1b1d23, 0x282c34, 0x21252b, 0x1d1f23, 0x3e4451],
        [0xd7dae0, 0xabb2bf, 0x7f848e],
        0x61afef,
        [
            0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd19a66,
        ],
    )
    .with_selection(hex(0x3e4451)),
};
