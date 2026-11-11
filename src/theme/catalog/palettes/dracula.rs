//! Published dracula palette values.
use super::super::*;

pub(in super::super) const DRACULA: Builtin = Builtin {
    id: "dracula",
    name: "Dracula",
    credit: "Dracula · draculatheme.com",
    palette: pal(
        true,
        [0x191a21, 0x282a36, 0x21222c, 0x191a21, 0x44475a],
        [0xf8f8f2, 0xc5c8d6, 0x7b88b8],
        0xbd93f9,
        [
            0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xffb86c,
        ],
    )
    .with_selection(hex(0x44475a))
    .with_ansi(ansi([
        0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2, 0x6272a4,
        0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
    ])),
};
