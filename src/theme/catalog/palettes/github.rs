//! Published github palette values.
use super::super::*;

pub(in super::super) const GITHUB_DARK: Builtin = Builtin {
    id: "github-dark",
    name: "GitHub Dark",
    credit: "GitHub Primer",
    palette: pal(
        true,
        [0x010409, 0x0d1117, 0x151b23, 0x0d1117, 0x3d444d],
        [0xf0f6fc, 0x9198a1, 0x7d8590],
        0x388bfd,
        [
            0xf85149, 0x3fb950, 0xd29922, 0x58a6ff, 0xbe8fff, 0x39c5cf, 0xf0883e,
        ],
    )
    .with_selection(hex(0x1f3a5f)),
};

pub(in super::super) const GITHUB_LIGHT: Builtin = Builtin {
    id: "github-light",
    name: "GitHub Light",
    credit: "GitHub Primer",
    palette: pal(
        false,
        [0xeaeef2, 0xffffff, 0xf6f8fa, 0xffffff, 0xd0d7de],
        [0x1f2328, 0x59636e, 0x818b98],
        0x0969da,
        [
            0xcf222e, 0x1a7f37, 0x9a6700, 0x0969da, 0x8250df, 0x1b7c83, 0xbc4c00,
        ],
    )
    .with_selection(hex(0xddf4ff)),
};
