//! Published tokyo palette values.
use super::super::*;

pub(in super::super) const TOKYO_NIGHT: Builtin = Builtin {
    id: "tokyo-night",
    name: "Tokyo Night",
    credit: "Tokyo Night · enkia, folke",
    palette: pal(
        true,
        [0x0c0e14, 0x1a1b26, 0x16161e, 0x0c0e14, 0x3b4261],
        [0xc0caf5, 0xa9b1d6, 0x737aa2],
        0x7aa2f7,
        [
            0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xff9e64,
        ],
    )
    .with_selection(hex(0x283457)),
};

pub(in super::super) const TOKYO_NIGHT_STORM: Builtin = Builtin {
    id: "tokyo-night-storm",
    name: "Tokyo Night Storm",
    credit: "Tokyo Night · enkia, folke",
    palette: pal(
        true,
        [0x1b1e2d, 0x24283b, 0x1f2335, 0x1b1e2d, 0x3b4261],
        [0xc0caf5, 0xa9b1d6, 0x737aa2],
        0x7aa2f7,
        [
            0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xff9e64,
        ],
    )
    .with_selection(hex(0x2e3c64)),
};

pub(in super::super) const TOKYO_NIGHT_DAY: Builtin = Builtin {
    id: "tokyo-night-day",
    name: "Tokyo Night Day",
    credit: "Tokyo Night · enkia, folke",
    palette: pal(
        false,
        [0xd0d5e3, 0xe1e2e7, 0xe9e9ec, 0xf2f2f5, 0xa8aecb],
        [0x3760bf, 0x6172b0, 0x848cb5],
        0x2e7de9,
        [
            0xf52a65, 0x587539, 0x8c6c3e, 0x2e7de9, 0x9854f1, 0x007197, 0xb15c00,
        ],
    ),
};
