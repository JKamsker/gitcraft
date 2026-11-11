//! Published horizon palette values.
use super::super::*;

pub(in super::super) const HORIZON: Builtin = Builtin {
    id: "horizon",
    name: "Horizon",
    credit: "Horizon · jolaleye",
    palette: pal(
        true,
        [0x16161c, 0x1c1e26, 0x232530, 0x1a1c23, 0x2e303e],
        [0xd5d8da, 0xbbbbbb, 0x6c6f93],
        0xe95678,
        [
            0xe95678, 0x29d398, 0xfab795, 0x26bbd9, 0xee64ac, 0x59e3e3, 0xf09483,
        ],
    )
    .with_selection(hex(0x2e303e)),
};
