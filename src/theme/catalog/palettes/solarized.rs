//! Published solarized palette values.
use super::super::*;

pub(in super::super) const SOLARIZED_DARK: Builtin = Builtin {
    id: "solarized-dark",
    name: "Solarized Dark",
    credit: "Solarized · Ethan Schoonover",
    palette: pal(
        true,
        [0x001e26, 0x002b36, 0x00212b, 0x073642, 0x1d4a54],
        [0x93a1a1, 0x839496, 0x586e75],
        0x268bd2,
        [
            0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xcb4b16,
        ],
    )
    .with_selection(hex(0x073642))
    .with_ansi(ansi([
        0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x586e75,
        0xcb4b16, 0x859900, 0xb58900, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
    ])),
};

pub(in super::super) const SOLARIZED_LIGHT: Builtin = Builtin {
    id: "solarized-light",
    name: "Solarized Light",
    credit: "Solarized · Ethan Schoonover",
    palette: pal(
        false,
        [0xe4ddc8, 0xfdf6e3, 0xeee8d5, 0xfffdf6, 0xd3cbb7],
        [0x073642, 0x586e75, 0x839496],
        0x268bd2,
        [
            0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xcb4b16,
        ],
    )
    .with_selection(hex(0xe4ddc8)),
};
