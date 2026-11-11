//! Published nord palette values.
use super::super::*;

pub(in super::super) const NORD: Builtin = Builtin {
    id: "nord",
    name: "Nord",
    credit: "Nord · Arctic Ice Studio",
    palette: pal(
        true,
        [0x242933, 0x2e3440, 0x3b4252, 0x2b303b, 0x4c566a],
        [0xeceff4, 0xd8dee9, 0x8590a6],
        0x88c0d0,
        [
            0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xd08770,
        ],
    )
    .with_selection(hex(0x434c5e))
    .with_ansi(ansi([
        0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0, 0x4c566a,
        0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
    ])),
};
