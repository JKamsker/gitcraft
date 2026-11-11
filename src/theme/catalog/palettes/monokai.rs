//! Published monokai palette values.
use super::super::*;

pub(in super::super) const MONOKAI: Builtin = Builtin {
    id: "monokai",
    name: "Monokai",
    credit: "Monokai · Wimer Hazenberg",
    palette: pal(
        true,
        [0x191a16, 0x272822, 0x1e1f1c, 0x31322a, 0x49483e],
        [0xf8f8f2, 0xcfcfc2, 0x90908a],
        0x66d9ef,
        [
            0xf92672, 0xa6e22e, 0xe6db74, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xfd971f,
        ],
    )
    .with_selection(hex(0x49483e)),
};
