//! Published gruvbox palette values.
use super::super::*;

pub(in super::super) const GRUVBOX_DARK: Builtin = Builtin {
    id: "gruvbox-dark",
    name: "Gruvbox Dark",
    credit: "Gruvbox · morhetz",
    palette: pal(
        true,
        [0x1d2021, 0x282828, 0x32302f, 0x1d2021, 0x504945],
        [0xebdbb2, 0xd5c4a1, 0x928374],
        0x83a598,
        [
            0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xfe8019,
        ],
    )
    .with_selection(hex(0x504945)),
};

pub(in super::super) const GRUVBOX_LIGHT: Builtin = Builtin {
    id: "gruvbox-light",
    name: "Gruvbox Light",
    credit: "Gruvbox · morhetz",
    palette: pal(
        false,
        [0xebdbb2, 0xfbf1c7, 0xf2e5bc, 0xf9f5d7, 0xd5c4a1],
        [0x3c3836, 0x504945, 0x7c6f64],
        0x076678,
        [
            0x9d0006, 0x79740e, 0xb57614, 0x076678, 0x8f3f71, 0x427b58, 0xaf3a03,
        ],
    )
    .with_selection(hex(0xd5c4a1)),
};
