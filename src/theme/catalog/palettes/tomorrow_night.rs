//! Published tomorrow_night palette values.
use super::super::*;

pub(in super::super) const TOMORROW_NIGHT: Builtin = Builtin {
    id: "tomorrow-night",
    name: "Tomorrow Night",
    credit: "Tomorrow · Chris Kempson",
    palette: pal(
        true,
        [0x161719, 0x1d1f21, 0x232528, 0x161719, 0x373b41],
        [0xc5c8c6, 0xb4b7b4, 0x969896],
        0x81a2be,
        [
            0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xde935f,
        ],
    )
    .with_selection(hex(0x373b41)),
};
