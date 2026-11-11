//! Published catppuccin palette values.
use super::super::*;

pub(in super::super) const CATPPUCCIN_MOCHA: Builtin = Builtin {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    credit: "Catppuccin · catppuccin.com",
    palette: pal(
        true,
        [0x11111b, 0x1e1e2e, 0x181825, 0x11111b, 0x45475a],
        [0xcdd6f4, 0xa6adc8, 0x7f849c],
        0xcba6f7,
        [
            0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xfab387,
        ],
    )
    .with_separator(hex(0x11111b))
    .with_selection(hex(0x45475a)),
};

pub(in super::super) const CATPPUCCIN_MACCHIATO: Builtin = Builtin {
    id: "catppuccin-macchiato",
    name: "Catppuccin Macchiato",
    credit: "Catppuccin · catppuccin.com",
    palette: pal(
        true,
        [0x181926, 0x24273a, 0x1e2030, 0x181926, 0x494d64],
        [0xcad3f5, 0xa5adcb, 0x8087a2],
        0xc6a0f6,
        [
            0xed8796, 0xa6da95, 0xeed49f, 0x8aadf4, 0xf5bde6, 0x8bd5ca, 0xf5a97f,
        ],
    )
    .with_separator(hex(0x181926))
    .with_selection(hex(0x494d64)),
};

pub(in super::super) const CATPPUCCIN_FRAPPE: Builtin = Builtin {
    id: "catppuccin-frappe",
    name: "Catppuccin Frappé",
    credit: "Catppuccin · catppuccin.com",
    palette: pal(
        true,
        [0x232634, 0x303446, 0x292c3c, 0x232634, 0x51576d],
        [0xc6d0f5, 0xa5adce, 0x838ba7],
        0xca9ee6,
        [
            0xe78284, 0xa6d189, 0xe5c890, 0x8caaee, 0xf4b8e4, 0x81c8be, 0xef9f76,
        ],
    )
    .with_separator(hex(0x232634))
    .with_selection(hex(0x51576d)),
};

pub(in super::super) const CATPPUCCIN_LATTE: Builtin = Builtin {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    credit: "Catppuccin · catppuccin.com",
    palette: pal(
        false,
        [0xdce0e8, 0xeff1f5, 0xe6e9ef, 0xf7f8fa, 0xbcc0cc],
        [0x4c4f69, 0x5c5f77, 0x8c8fa1],
        0x8839ef,
        [
            0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xfe640b,
        ],
    )
    .with_selection(hex(0xccd0da)),
};
